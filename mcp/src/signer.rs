//! The signing identity: the user's exported key file, unlocked on first
//! use and kept for the life of the process.
//!
//! Argon2id makes unlocking deliberately slow, so it happens at most once,
//! and only when a tool actually needs to sign or sign in. The key never
//! reaches a tool's output or a log line.

use std::fs::File;
use std::io::Read as _;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crisp_core::keystore::{self, KeyFile};
use crisp_core::sign::SecretKeySet;
use secrecy::ExposeSecret;
#[cfg(unix)]
use tracing::warn;
use tracing::{debug, info, info_span};
use zeroize::Zeroizing;

use crate::config::{KeyConfig, PassphraseSource};

/// The variable naming the key file, as it appears in error messages.
const KEY_FILE: &str = "CRISP_KEY_FILE";
/// The variable naming the passphrase file, as it appears in error messages.
const PASSPHRASE_FILE: &str = "CRISP_KEY_PASSPHRASE_FILE";

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
    /// passphrase file cannot be read or (on Unix) is readable or writable by
    /// group or other, or the passphrase is wrong. A failure
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

/// Opens a secret file named by `variable`, refusing it on Unix when group or
/// other may read or write it.
///
/// The mode is read from the opened handle, so the file checked is the file
/// read, and a symlink is judged by its target.
fn open_private(variable: &str, path: &Path) -> Result<File, SignerError> {
    let file = File::open(path).map_err(|e| unreadable(variable, path, &e))?;
    #[cfg(unix)]
    refuse_loose_mode(variable, path, &file)?;
    Ok(file)
}

/// The error for a secret file that could not be opened or read.
fn unreadable(variable: &str, path: &Path, error: &std::io::Error) -> SignerError {
    SignerError(format!(
        "could not read {variable} {}: {error}",
        path.display()
    ))
}

/// Refuses `file` when its mode grants group or other any read or write bit.
#[cfg(unix)]
fn refuse_loose_mode(variable: &str, path: &Path, file: &File) -> Result<(), SignerError> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = file
        .metadata()
        .map_err(|e| unreadable(variable, path, &e))?
        .permissions()
        .mode()
        & 0o7777;
    debug!(variable, path = %path.display(), mode = %format_args!("{mode:04o}"), "checked secret file mode");
    if mode & 0o066 == 0 {
        return Ok(());
    }
    warn!(
        variable,
        path = %path.display(),
        mode = %format_args!("{mode:04o}"),
        "refusing a secret file that group or other can read or write"
    );
    Err(SignerError(format!(
        "{variable} {path} is readable or writable by group or other (mode {mode:04o}); \
         restrict it with `chmod 600 {path}` and try again",
        path = path.display()
    )))
}

/// Reads and unwraps the key file.
fn unlock(config: &KeyConfig) -> Result<SecretKeySet, SignerError> {
    let span = info_span!("unlock_key", key_file = %config.key_file.display());
    let _guard = span.enter();
    let mut bytes = Vec::new();
    open_private(KEY_FILE, &config.key_file)?
        .read_to_end(&mut bytes)
        .map_err(|e| unreadable(KEY_FILE, &config.key_file, &e))?;
    let file = KeyFile::from_bytes(&bytes)
        .map_err(|e| SignerError(format!("CRISP_KEY_FILE is not a Crisp key file: {e}")))?;
    // Held as `Zeroizing`, so the passphrase is wiped when dropped, and
    // trimmed in place so no unwiped copy is left behind.
    let passphrase: Zeroizing<String> = match &config.passphrase {
        PassphraseSource::Inline(secret) => Zeroizing::new(secret.expose_secret().to_owned()),
        PassphraseSource::File(path) => {
            let mut text = Zeroizing::new(String::new());
            open_private(PASSPHRASE_FILE, path)?
                .read_to_string(&mut text)
                .map_err(|e| unreadable(PASSPHRASE_FILE, path, &e))?;
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
        write_private(&path, &wrapped.to_bytes().expect("bytes"));
        (path, expected)
    }

    /// Writes `contents` to `path` readable and writable by its owner only.
    fn write_private(path: &std::path::Path, contents: &[u8]) {
        std::fs::write(path, contents).expect("write");
        #[cfg(unix)]
        set_mode(path, 0o600);
    }

    #[cfg(unix)]
    fn set_mode(path: &std::path::Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
    }

    #[test]
    fn a_key_file_unlocks_with_a_passphrase_file_and_is_cached() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (path, expected) = key_file(dir.path(), "correct horse");
        let pass = dir.path().join("pass");
        write_private(&pass, b"correct horse\n");
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
        write_private(&garbage, b"not a key");
        let bad = refused(&Signer::new(Some(KeyConfig {
            key_file: garbage,
            passphrase: PassphraseSource::Inline(SecretString::from("whatever!")),
        })));
        assert!(bad.0.contains("not a Crisp key file"), "{bad}");
    }

    /// The modes the check must refuse: each grants group or other a read or
    /// write bit.
    #[cfg(unix)]
    const LOOSE: [u32; 4] = [0o640, 0o604, 0o660, 0o602];

    /// A key file and passphrase file for "correct horse", both `0600`.
    #[cfg(unix)]
    fn private_pair(dir: &std::path::Path) -> (KeyConfig, std::path::PathBuf, std::path::PathBuf) {
        let (key, _) = key_file(dir, "correct horse");
        let pass = dir.join("pass");
        write_private(&pass, b"correct horse\n");
        let config = KeyConfig {
            key_file: key.clone(),
            passphrase: PassphraseSource::File(pass.clone()),
        };
        (config, key, pass)
    }

    #[cfg(unix)]
    #[test]
    fn owner_only_modes_unlock() {
        for mode in [0o600, 0o400] {
            let dir = tempfile::tempdir().expect("tempdir");
            let (config, key, pass) = private_pair(dir.path());
            set_mode(&key, mode);
            set_mode(&pass, mode);
            unlocked(&Signer::new(Some(config)));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_loose_key_file_is_refused_with_a_chmod_hint() {
        for mode in LOOSE {
            let dir = tempfile::tempdir().expect("tempdir");
            let (config, key, _) = private_pair(dir.path());
            set_mode(&key, mode);
            let error = refused(&Signer::new(Some(config)));
            assert!(error.0.starts_with("CRISP_KEY_FILE "), "{error}");
            assert!(error.0.contains(&key.display().to_string()), "{error}");
            assert!(error.0.contains(&format!("mode {mode:04o}")), "{error}");
            assert!(
                error.0.contains(&format!("chmod 600 {}", key.display())),
                "{error}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_loose_passphrase_file_is_refused_with_a_chmod_hint() {
        for mode in LOOSE {
            let dir = tempfile::tempdir().expect("tempdir");
            let (config, _, pass) = private_pair(dir.path());
            set_mode(&pass, mode);
            let error = refused(&Signer::new(Some(config)));
            assert!(error.0.starts_with("CRISP_KEY_PASSPHRASE_FILE "), "{error}");
            assert!(error.0.contains(&format!("mode {mode:04o}")), "{error}");
            assert!(
                error.0.contains(&format!("chmod 600 {}", pass.display())),
                "{error}"
            );
            assert!(!error.0.contains("correct horse"), "{error}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_judged_by_its_target_and_a_fixed_mode_retries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (mut config, key, _) = private_pair(dir.path());
        let link = dir.path().join("link.crispkey");
        std::os::unix::fs::symlink(&key, &link).expect("symlink");
        config.key_file = link;
        set_mode(&key, 0o644);
        let signer = Signer::new(Some(config));
        let error = refused(&signer);
        assert!(error.0.contains("mode 0644"), "{error}");
        set_mode(&key, 0o600);
        unlocked(&signer);
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
