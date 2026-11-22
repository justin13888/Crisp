//! The signing identity: the user's exported key file, unlocked on first
//! use and kept for the life of the process.
//!
//! Argon2id makes unlocking deliberately slow, so it happens at most once,
//! and only when a tool actually needs to sign or sign in. The key never
//! reaches a tool's output or a log line.

use std::sync::{Arc, Mutex};

use crisp_core::keystore::{self, KeyFile};
use crisp_core::sign::SecretKeySet;
use secrecy::ExposeSecret;
use tracing::{debug, info, info_span};
use zeroize::Zeroizing;

use crate::config::{KeyConfig, PassphraseSource};

/// Why the identity could not be unlocked. Messages name the variable to fix.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SignerError(pub String);

/// Where the secret key comes from.
enum Source {
    /// No key configured.
    Missing,
    /// A key file to unlock on first use.
    File(KeyConfig),
}

/// The lazily unlocked signing identity.
pub struct Signer {
    source: Source,
    unlocked: Mutex<Option<Arc<SecretKeySet>>>,
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Signer").finish_non_exhaustive()
    }
}

impl Signer {
    /// A signer that unlocks `config` on first use, or reports that no key is
    /// configured when `config` is `None`.
    #[must_use]
    pub fn new(config: Option<KeyConfig>) -> Self {
        Self {
            source: config.map_or(Source::Missing, Source::File),
            unlocked: Mutex::new(None),
        }
    }

    /// A signer over an already unlocked key (tests, embedding).
    #[must_use]
    pub fn from_secret(secret: SecretKeySet) -> Self {
        Self {
            source: Source::Missing,
            unlocked: Mutex::new(Some(Arc::new(secret))),
        }
    }

    /// The unlocked key, unlocking it on the first call.
    ///
    /// # Errors
    ///
    /// Returns a [`SignerError`] when no key is configured, the key or
    /// passphrase file cannot be read, or the passphrase is wrong. A failure
    /// is not cached, so fixing the file and retrying works.
    pub fn secret(&self) -> Result<Arc<SecretKeySet>, SignerError> {
        let mut slot = self
            .unlocked
            .lock()
            .map_err(|_| SignerError("the signer lock was poisoned".into()))?;
        if let Some(secret) = slot.as_ref() {
            return Ok(Arc::clone(secret));
        }
        let Source::File(config) = &self.source else {
            return Err(SignerError(
                "no signing key is configured: export your key from the Crisp app and set \
                 CRISP_KEY_FILE and CRISP_KEY_PASSPHRASE_FILE"
                    .into(),
            ));
        };
        let secret = Arc::new(unlock(config)?);
        *slot = Some(Arc::clone(&secret));
        Ok(secret)
    }

    /// [`Signer::secret`] on a blocking thread, so the deliberately slow
    /// Argon2id derivation never stalls the async runtime.
    ///
    /// # Errors
    ///
    /// As [`Signer::secret`].
    pub async fn unlock(self: &Arc<Self>) -> Result<Arc<SecretKeySet>, SignerError> {
        let signer = Arc::clone(self);
        tokio::task::spawn_blocking(move || signer.secret())
            .await
            .map_err(|e| SignerError(format!("unlocking the key failed: {e}")))?
    }
}

/// Reads and unwraps the key file.
fn unlock(config: &KeyConfig) -> Result<SecretKeySet, SignerError> {
    let span = info_span!("unlock_key", key_file = %config.key_file.display());
    let _guard = span.enter();
    let bytes = std::fs::read(&config.key_file).map_err(|e| {
        SignerError(format!(
            "could not read CRISP_KEY_FILE {}: {e}",
            config.key_file.display()
        ))
    })?;
    let file = KeyFile::from_bytes(&bytes)
        .map_err(|e| SignerError(format!("CRISP_KEY_FILE is not a Crisp key file: {e}")))?;
    // Held as `Zeroizing`, so the passphrase is wiped when dropped, and
    // trimmed in place so no unwiped copy is left behind.
    let passphrase: Zeroizing<String> = match &config.passphrase {
        PassphraseSource::Inline(secret) => Zeroizing::new(secret.expose_secret().to_owned()),
        PassphraseSource::File(path) => {
            let mut text = Zeroizing::new(std::fs::read_to_string(path).map_err(|e| {
                SignerError(format!(
                    "could not read CRISP_KEY_PASSPHRASE_FILE {}: {e}",
                    path.display()
                ))
            })?);
            let kept = text.trim_end_matches(['\r', '\n']).len();
            text.truncate(kept);
            text
        }
    };
    debug!("deriving the key-wrapping key");
    let seed = keystore::unwrap(&file, &passphrase).map_err(|e| {
        SignerError(format!(
            "could not unlock CRISP_KEY_FILE (wrong passphrase?): {e}"
        ))
    })?;
    let secret = seed.keyset();
    info!(
        user_id = secret.user_id().map(|u| u.0).unwrap_or_default(),
        "signing key unlocked"
    );
    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crisp_core::sign::IdentitySeed;
    use secrecy::SecretString;

    fn unlocked(signer: &Signer) -> Arc<SecretKeySet> {
        match signer.secret() {
            Ok(secret) => secret,
            Err(e) => panic!("expected to unlock: {e}"),
        }
    }

    fn refused(signer: &Signer) -> SignerError {
        match signer.secret() {
            Ok(_) => panic!("expected the signer to refuse"),
            Err(e) => e,
        }
    }

    fn key_file(dir: &std::path::Path, passphrase: &str) -> (std::path::PathBuf, String) {
        let seed = IdentitySeed::generate().expect("seed");
        let expected = seed.keyset().user_id().expect("uid").0;
        let wrapped = keystore::wrap(&seed, passphrase).expect("wrap");
        let path = dir.join("me.crispkey");
        std::fs::write(&path, wrapped.to_bytes().expect("bytes")).expect("write");
        (path, expected)
    }

    #[test]
    fn a_key_file_unlocks_with_a_passphrase_file_and_is_cached() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, expected) = key_file(dir.path(), "correct horse");
        let pass = dir.path().join("pass");
        std::fs::write(&pass, "correct horse\n").expect("write");
        let signer = Signer::new(Some(KeyConfig {
            key_file: path.clone(),
            passphrase: PassphraseSource::File(pass),
        }));
        let first = unlocked(&signer);
        assert_eq!(first.user_id().expect("uid").0, expected);
        std::fs::remove_file(&path).expect("remove");
        let second = unlocked(&signer);
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn a_wrong_passphrase_is_a_clean_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, _) = key_file(dir.path(), "correct horse");
        let signer = Signer::new(Some(KeyConfig {
            key_file: path,
            passphrase: PassphraseSource::Inline(SecretString::from("wrong horse")),
        }));
        let error = refused(&signer);
        assert!(error.0.contains("wrong passphrase"), "{error}");
    }

    #[test]
    fn missing_files_and_keys_name_the_variable() {
        let none = refused(&Signer::new(None));
        assert!(none.0.contains("CRISP_KEY_FILE"), "{none}");
        let dir = tempfile::tempdir().expect("tempdir");
        let absent = refused(&Signer::new(Some(KeyConfig {
            key_file: dir.path().join("absent"),
            passphrase: PassphraseSource::Inline(SecretString::from("whatever!")),
        })));
        assert!(
            absent.0.contains("could not read CRISP_KEY_FILE"),
            "{absent}"
        );
        let (path, _) = key_file(dir.path(), "correct horse");
        let no_pass = refused(&Signer::new(Some(KeyConfig {
            key_file: path,
            passphrase: PassphraseSource::File(dir.path().join("absent-pass")),
        })));
        assert!(no_pass.0.contains("CRISP_KEY_PASSPHRASE_FILE"), "{no_pass}");
        let garbage = dir.path().join("garbage");
        std::fs::write(&garbage, b"not a key").expect("write");
        let bad = refused(&Signer::new(Some(KeyConfig {
            key_file: garbage,
            passphrase: PassphraseSource::Inline(SecretString::from("whatever!")),
        })));
        assert!(bad.0.contains("not a Crisp key file"), "{bad}");
    }

    #[test]
    fn debug_output_reveals_nothing() {
        let shown = format!(
            "{:?}",
            Signer::from_secret(SecretKeySet::from_seed(&[4u8; 32]))
        );
        assert_eq!(shown, "Signer { .. }");
    }

    #[test]
    fn a_given_secret_is_used_as_is() {
        let secret = SecretKeySet::from_seed(&[3u8; 32]);
        let expected = secret.user_id().expect("uid");
        let signer = Signer::from_secret(secret);
        assert_eq!(unlocked(&signer).user_id().expect("uid"), expected);
    }
}
