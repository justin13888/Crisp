//! The signing identity: generated in WASM, stored only as a
//! passphrase-wrapped key file, and never exposed to JavaScript.

use crisp_core::author::seal;
use crisp_core::keystore::{KeyFile, unwrap, wrap};
use crisp_core::model::payload::BundlePayload;
use crisp_core::sign::{IdentitySeed, SecretKeySet, login_message};
use wasm_bindgen::prelude::*;

use crate::boundary::{OrJs, failure, from_js, timestamp, to_js};

/// An unlocked identity. Holds secret key material inside WASM memory;
/// call `free()` when locking.
#[wasm_bindgen]
pub struct Signer {
    seed: IdentitySeed,
    keys: SecretKeySet,
    user_id: String,
}

impl Signer {
    fn from_seed(seed: IdentitySeed) -> Result<Self, JsValue> {
        let keys = seed.keyset();
        let user_id = keys.user_id().or_js()?.0;
        Ok(Self {
            seed,
            keys,
            user_id,
        })
    }
}

#[wasm_bindgen]
impl Signer {
    /// Creates a brand-new random identity.
    ///
    /// # Errors
    ///
    /// Throws if the system RNG is unavailable.
    pub fn generate() -> Result<Signer, JsValue> {
        Self::from_seed(IdentitySeed::generate().or_js()?)
    }

    /// Unlocks a `.crispkey` file. Takes about a second (Argon2id, 64 MiB).
    ///
    /// # Errors
    ///
    /// Throws if the file is not a key file or the passphrase is wrong.
    pub fn unlock(key_file: &[u8], passphrase: &str) -> Result<Signer, JsValue> {
        let file = KeyFile::from_bytes(key_file).or_js()?;
        Self::from_seed(unwrap(&file, passphrase).or_js()?)
    }

    /// Wraps this identity under `passphrase` as `.crispkey` bytes.
    ///
    /// # Errors
    ///
    /// Throws if the passphrase is too short or encryption fails.
    pub fn wrap(&self, passphrase: &str) -> Result<Vec<u8>, JsValue> {
        wrap(&self.seed, passphrase).or_js()?.to_bytes().or_js()
    }

    /// This identity's `usr…` id.
    #[wasm_bindgen(getter, js_name = userId)]
    #[must_use]
    pub fn user_id(&self) -> String {
        self.user_id.clone()
    }

    /// Seals a draft into a signed `.crisp` file. Pass the bytes of the file
    /// being edited as `previous`: the owner's edit becomes a revision,
    /// anyone else's a fork.
    ///
    /// # Errors
    ///
    /// Throws with the draft's `issues` if it would not verify.
    pub fn seal(
        &self,
        #[wasm_bindgen(unchecked_param_type = "BundlePayload")] draft: JsValue,
        #[wasm_bindgen(js_name = displayName)] display_name: &str,
        now: f64,
        previous: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, JsValue> {
        let draft: BundlePayload = from_js(draft, "draft")?;
        seal(
            &draft,
            &self.keys,
            display_name,
            timestamp(now)?,
            previous.as_deref(),
        )
        .map_err(|e| {
            let error = crate::boundary::core_error(&e);
            let issues = crisp_core::author::validate_draft(&draft);
            if let Ok(issues) = to_js(&issues) {
                let _ = js_sys::Reflect::set(&error, &"issues".into(), &issues);
            }
            error
        })
    }

    /// Signs a self-asserted account profile for server registration.
    ///
    /// # Errors
    ///
    /// Throws if a field is out of range or signing fails.
    #[wasm_bindgen(js_name = signProfile)]
    pub fn sign_profile(
        &self,
        username: &str,
        display_name: &str,
        now: f64,
    ) -> Result<Vec<u8>, JsValue> {
        crisp_core::profile::sign_profile(&self.keys, username, display_name, timestamp(now)?)
            .or_js()
    }

    /// Answers a server login challenge (spec §8.7): signs the
    /// audience-bound `crisp/login/1` message for `nonce` (hex).
    ///
    /// # Errors
    ///
    /// Throws if the nonce is not hex or signing fails.
    #[wasm_bindgen(js_name = signLogin)]
    pub fn sign_login(&self, audience: &str, nonce: &str) -> Result<Vec<u8>, JsValue> {
        let nonce = decode_hex(nonce)
            .ok_or_else(|| failure("the challenge nonce is not hex", None, None))?;
        self.keys
            .sign(&login_message(audience, &nonce).or_js()?)
            .or_js()
    }
}

/// Reads the `usr…` id of a key file without unlocking it.
///
/// # Errors
///
/// Throws if the bytes are not a supported key file.
#[wasm_bindgen(js_name = keyFileUserId)]
pub fn key_file_user_id(key_file: &[u8]) -> Result<String, JsValue> {
    Ok(KeyFile::from_bytes(key_file).or_js()?.uid.0)
}

/// Decodes lowercase or uppercase hex.
pub(crate) fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Encodes bytes as lowercase hex.
pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
