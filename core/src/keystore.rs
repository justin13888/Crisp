//! Passphrase-wrapped identity key files (`.crispkey`, spec §8.6).
//!
//! An identity is a random 32-byte [`IdentitySeed`]. It is never derived from
//! a passphrase — the passphrase only *wraps* it for storage and backup:
//!
//! ```text
//! key = Argon2id(passphrase, salt; m = 64 MiB, t = 3, p = 1)  (32 bytes)
//! ct  = XChaCha20-Poly1305(key, nonce, seed, aad = .cbor key-file-header)
//! ```
//!
//! The salt and nonce are random per wrap, so an attacker holding only a
//! user's public keys has nothing to brute-force; an attacker holding the
//! key file faces a memory-hard KDF. The header (including the `usr…` id) is
//! authenticated, so a file cannot be relabelled to another identity.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::codec::{from_canonical_cbor, to_canonical_cbor};
use crate::error::{CrispError, Result};
use crate::model::ids::UserId;
use crate::sign::{IdentitySeed, SEED_LEN};

/// The key-file format version.
pub const KEY_FILE_VERSION: u64 = 1;
/// Argon2id memory cost for new key files, in KiB (64 MiB).
pub const ARGON2_M_COST_KIB: u32 = 65_536;
/// Argon2id time cost (passes) for new key files.
pub const ARGON2_T_COST: u32 = 3;
/// Argon2id parallelism (lanes) for new key files.
pub const ARGON2_P_COST: u32 = 1;
/// The weakest Argon2id memory cost a key file may declare, in KiB.
const MIN_M_COST_KIB: u32 = 19_456;
/// The strongest Argon2id memory cost a key file may declare, in KiB (1 GiB),
/// so a hostile file cannot make unlocking exhaust memory.
const MAX_M_COST_KIB: u32 = 1_048_576;
/// The longest a key file may make unlocking run, in Argon2 passes.
const MAX_T_COST: u32 = 16;
/// The salt length, in bytes.
const SALT_LEN: usize = 16;
/// The XChaCha20 nonce length, in bytes.
const NONCE_LEN: usize = 24;
/// The minimum passphrase length, in characters.
pub const MIN_PASSPHRASE_CHARS: usize = 8;

/// Argon2id parameters recorded in a key file (`crisp-v1.cddl`: `key-file.kdf`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KdfParams {
    /// Always `"argon2id"`.
    pub alg: String,
    /// Memory cost, in KiB.
    pub m: u32,
    /// Time cost (passes).
    pub t: u32,
    /// Parallelism (lanes).
    pub p: u32,
    /// The random salt.
    #[serde(with = "serde_bytes")]
    pub salt: Vec<u8>,
}

/// The authenticated part of a key file (`crisp-v1.cddl`: `key-file-header`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyFileHeader {
    crisp_key: u64,
    uid: UserId,
    kdf: KdfParams,
    aead: String,
    #[serde(with = "serde_bytes")]
    nonce: Vec<u8>,
}

/// A passphrase-wrapped identity (`crisp-v1.cddl`: `key-file`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyFile {
    /// Always [`KEY_FILE_VERSION`].
    pub crisp_key: u64,
    /// The `usr…` id of the wrapped identity — readable without unlocking.
    pub uid: UserId,
    /// The key-derivation parameters.
    pub kdf: KdfParams,
    /// Always `"xchacha20poly1305"`.
    pub aead: String,
    /// The AEAD nonce.
    #[serde(with = "serde_bytes")]
    pub nonce: Vec<u8>,
    /// The wrapped seed plus its authentication tag.
    #[serde(with = "serde_bytes")]
    pub ct: Vec<u8>,
}

impl KeyFile {
    /// The header bytes the AEAD authenticates.
    fn aad(&self) -> Result<Vec<u8>> {
        to_canonical_cbor(&KeyFileHeader {
            crisp_key: self.crisp_key,
            uid: self.uid.clone(),
            kdf: self.kdf.clone(),
            aead: self.aead.clone(),
            nonce: self.nonce.clone(),
        })
    }

    /// Encodes this key file as canonical CBOR (the `.crispkey` bytes).
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        to_canonical_cbor(self)
    }

    /// Decodes `.crispkey` bytes and checks the header is one this reader
    /// supports, without unlocking it.
    ///
    /// # Errors
    ///
    /// Returns [`CrispError::Input`] if the bytes are not a supported key file.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let file: KeyFile = from_canonical_cbor(bytes)
            .map_err(|e| CrispError::input(format!("not a Crisp key file: {e}")))?;
        file.check_header()?;
        Ok(file)
    }

    /// Validates the header fields against what this reader supports.
    fn check_header(&self) -> Result<()> {
        let bad = |what: &str| Err(CrispError::input(format!("unsupported key file: {what}")));
        if self.crisp_key != KEY_FILE_VERSION {
            return bad("version");
        }
        if self.kdf.alg != "argon2id" || self.aead != "xchacha20poly1305" {
            return bad("algorithm");
        }
        if !(MIN_M_COST_KIB..=MAX_M_COST_KIB).contains(&self.kdf.m)
            || !(1..=MAX_T_COST).contains(&self.kdf.t)
            || self.kdf.p != 1
        {
            return bad("KDF parameters");
        }
        if self.kdf.salt.len() != SALT_LEN || self.nonce.len() != NONCE_LEN {
            return bad("salt or nonce length");
        }
        if self.ct.len() != SEED_LEN + 16 {
            return bad("ciphertext length");
        }
        Ok(())
    }
}

/// Derives the 32-byte wrapping key from a passphrase.
fn wrapping_key(passphrase: &str, kdf: &KdfParams) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(kdf.m, kdf.t, kdf.p, Some(32))
        .map_err(|e| CrispError::crypto(format!("invalid Argon2 parameters: {e}")))?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), &kdf.salt, key.as_mut())
        .map_err(|e| CrispError::crypto(format!("Argon2 derivation failed: {e}")))?;
    Ok(key)
}

/// Reads a stored nonce as an XChaCha20 nonce.
fn nonce(bytes: &[u8]) -> Result<XNonce> {
    XNonce::try_from(bytes).map_err(|_| CrispError::input("key file nonce has the wrong length"))
}

/// Fills a buffer from the system CSPRNG.
fn random<const N: usize>() -> Result<[u8; N]> {
    let mut out = [0u8; N];
    getrandom::fill(&mut out)
        .map_err(|e| CrispError::crypto(format!("system RNG unavailable: {e}")))?;
    Ok(out)
}

/// Wraps `seed` under `passphrase` into a new [`KeyFile`].
///
/// # Errors
///
/// Returns [`CrispError::Input`] if the passphrase is shorter than
/// [`MIN_PASSPHRASE_CHARS`], or [`CrispError::Crypto`] if key derivation,
/// encryption, or the RNG fails.
pub fn wrap(seed: &IdentitySeed, passphrase: &str) -> Result<KeyFile> {
    if passphrase.chars().count() < MIN_PASSPHRASE_CHARS {
        return Err(CrispError::input(format!(
            "passphrase must be at least {MIN_PASSPHRASE_CHARS} characters"
        )));
    }
    let mut file = KeyFile {
        crisp_key: KEY_FILE_VERSION,
        uid: seed.keyset().user_id()?,
        kdf: KdfParams {
            alg: "argon2id".to_owned(),
            m: ARGON2_M_COST_KIB,
            t: ARGON2_T_COST,
            p: ARGON2_P_COST,
            salt: random::<SALT_LEN>()?.to_vec(),
        },
        aead: "xchacha20poly1305".to_owned(),
        nonce: random::<NONCE_LEN>()?.to_vec(),
        ct: Vec::new(),
    };
    let key = wrapping_key(passphrase, &file.kdf)?;
    let aad = file.aad()?;
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    file.ct = cipher
        .encrypt(
            &nonce(&file.nonce)?,
            Payload {
                msg: seed.as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|_| CrispError::crypto("key file encryption failed"))?;
    Ok(file)
}

/// Unwraps a [`KeyFile`] with `passphrase`, returning the identity seed.
///
/// # Errors
///
/// Returns [`CrispError::Input`] if the passphrase is wrong or the file was
/// tampered with (the two are indistinguishable by design), or if the
/// unwrapped seed does not own the file's `uid`.
pub fn unwrap(file: &KeyFile, passphrase: &str) -> Result<IdentitySeed> {
    file.check_header()?;
    let key = wrapping_key(passphrase, &file.kdf)?;
    let cipher = XChaCha20Poly1305::new(&Key::from(*key));
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                &nonce(&file.nonce)?,
                Payload {
                    msg: &file.ct,
                    aad: &file.aad()?,
                },
            )
            .map_err(|_| CrispError::input("wrong passphrase, or the key file is damaged"))?,
    );
    let bytes: [u8; SEED_LEN] = plain
        .as_slice()
        .try_into()
        .map_err(|_| CrispError::input("key file holds a seed of the wrong length"))?;
    let seed = IdentitySeed::from_bytes(bytes);
    if seed.keyset().user_id()? != file.uid {
        return Err(CrispError::input("key file does not match its identity"));
    }
    Ok(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASS: &str = "correct horse battery";
    const DAMAGED: &str = "wrong passphrase, or the key file is damaged";

    /// The message of an expected [`CrispError::Input`].
    fn input_message<T: std::fmt::Debug>(result: Result<T>) -> String {
        match result {
            Err(CrispError::Input(message)) => message,
            other => panic!("expected an input error, got {other:?}"),
        }
    }

    /// A header-valid key file with placeholder contents, for checks that
    /// never reach the KDF.
    fn placeholder() -> KeyFile {
        KeyFile {
            crisp_key: KEY_FILE_VERSION,
            uid: UserId("usr-placeholder".to_owned()),
            kdf: KdfParams {
                alg: "argon2id".to_owned(),
                m: ARGON2_M_COST_KIB,
                t: ARGON2_T_COST,
                p: ARGON2_P_COST,
                salt: vec![0; SALT_LEN],
            },
            aead: "xchacha20poly1305".to_owned(),
            nonce: vec![0; NONCE_LEN],
            ct: vec![0; SEED_LEN + 16],
        }
    }

    /// One edit to a key file's fields.
    type Edit = fn(&mut KeyFile);

    /// The header verdict for the placeholder after `edit`.
    fn header_after(edit: impl FnOnce(&mut KeyFile)) -> Result<()> {
        let mut file = placeholder();
        edit(&mut file);
        file.check_header()
    }

    #[test]
    fn wrap_then_unwrap_round_trips_through_bytes() {
        let seed = IdentitySeed::generate().expect("seed");
        let file = wrap(&seed, PASS).expect("wrap");
        assert_eq!(file.uid, seed.keyset().user_id().expect("uid"));
        let decoded = KeyFile::from_bytes(&file.to_bytes().expect("encode")).expect("decode");
        let back = unwrap(&decoded, PASS).expect("unwrap");
        assert_eq!(back.as_bytes(), seed.as_bytes());
    }

    #[test]
    fn the_wrong_passphrase_is_rejected() {
        let seed = IdentitySeed::generate().expect("seed");
        let file = wrap(&seed, PASS).expect("wrap");
        assert_eq!(
            input_message(unwrap(&file, "incorrect horse battery")),
            DAMAGED
        );
    }

    #[test]
    fn a_relabelled_identity_is_rejected() {
        let seed = IdentitySeed::generate().expect("seed");
        let mut file = wrap(&seed, PASS).expect("wrap");
        file.uid = IdentitySeed::generate()
            .expect("seed")
            .keyset()
            .user_id()
            .expect("uid");
        // The uid is in the authenticated header, so the AEAD refuses it.
        assert_eq!(input_message(unwrap(&file, PASS)), DAMAGED);
    }

    #[test]
    fn wrapping_is_salted_and_short_passphrases_are_refused() {
        let seed = IdentitySeed::generate().expect("seed");
        let a = wrap(&seed, PASS).expect("wrap");
        let b = wrap(&seed, PASS).expect("wrap");
        assert_ne!(a.kdf.salt, b.kdf.salt);
        assert_ne!(a.ct, b.ct);
        assert_eq!(
            input_message(wrap(&seed, "short")),
            "passphrase must be at least 8 characters"
        );
    }

    #[test]
    fn the_passphrase_minimum_is_eight_characters_inclusive() {
        let seed = IdentitySeed::generate().expect("seed");
        let short = "a".repeat(MIN_PASSPHRASE_CHARS - 1);
        assert!(input_message(wrap(&seed, &short)).contains("at least 8 characters"));
        // Characters, not bytes: seven two-byte characters are still short.
        let wide = "é".repeat(MIN_PASSPHRASE_CHARS - 1);
        assert_eq!(wide.len(), 2 * (MIN_PASSPHRASE_CHARS - 1));
        assert!(input_message(wrap(&seed, &wide)).contains("at least 8 characters"));
        let exact = "a".repeat(MIN_PASSPHRASE_CHARS);
        let file = wrap(&seed, &exact).expect("an 8-character passphrase is accepted");
        assert_eq!(
            unwrap(&file, &exact).expect("unwrap").as_bytes(),
            seed.as_bytes()
        );
    }

    #[test]
    fn hostile_kdf_parameters_are_refused_before_any_work() {
        let seed = IdentitySeed::generate().expect("seed");
        let mut file = wrap(&seed, PASS).expect("wrap");
        file.kdf.m = u32::MAX;
        let refused = "unsupported key file: KDF parameters";
        assert_eq!(input_message(unwrap(&file, PASS)), refused);
        assert_eq!(
            input_message(KeyFile::from_bytes(&file.to_bytes().expect("encode"))),
            refused
        );
    }

    #[test]
    fn the_placeholder_header_is_supported() {
        assert!(header_after(|_| {}).is_ok());
    }

    #[test]
    fn kdf_bounds_are_inclusive() {
        let accepted: [Edit; 4] = [
            |f| f.kdf.m = MIN_M_COST_KIB,
            |f| f.kdf.m = MAX_M_COST_KIB,
            |f| f.kdf.t = 1,
            |f| f.kdf.t = MAX_T_COST,
        ];
        for edit in accepted {
            assert!(header_after(edit).is_ok());
        }
        let refused: [Edit; 6] = [
            |f| f.kdf.m = MIN_M_COST_KIB - 1,
            |f| f.kdf.m = MAX_M_COST_KIB + 1,
            |f| f.kdf.t = 0,
            |f| f.kdf.t = MAX_T_COST + 1,
            |f| f.kdf.p = 0,
            |f| f.kdf.p = 2,
        ];
        for edit in refused {
            assert_eq!(
                input_message(header_after(edit)),
                "unsupported key file: KDF parameters"
            );
        }
    }

    #[test]
    fn each_unsupported_header_field_is_named() {
        let cases: [(Edit, &str); 7] = [
            (|f| f.crisp_key = KEY_FILE_VERSION + 1, "version"),
            (|f| f.kdf.alg = "scrypt".to_owned(), "algorithm"),
            (|f| f.aead = "aes-256-gcm".to_owned(), "algorithm"),
            (
                |f| f.kdf.salt.truncate(SALT_LEN - 1),
                "salt or nonce length",
            ),
            (|f| f.nonce.push(0), "salt or nonce length"),
            (|f| f.ct.truncate(SEED_LEN + 15), "ciphertext length"),
            (|f| f.ct.push(0), "ciphertext length"),
        ];
        for (edit, what) in cases {
            assert_eq!(
                input_message(header_after(edit)),
                format!("unsupported key file: {what}")
            );
        }
    }

    #[test]
    fn non_key_file_bytes_are_refused() {
        assert!(input_message(KeyFile::from_bytes(&[0xa0])).starts_with("not a Crisp key file"));
    }
}
