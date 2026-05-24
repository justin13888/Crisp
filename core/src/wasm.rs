// `wasm-bindgen`'s generated glue requires `unsafe`; allow it just here.
#![allow(unsafe_code)]
//! The minimal `wasm-bindgen` API surface for the web client.
//!
//! This module is compiled only under the `wasm` feature. It is a thin,
//! value-oriented bridge: JavaScript passes byte arrays and plain objects in
//! and gets plain objects out. Secret key material never crosses the
//! boundary — it stays inside an opaque [`IdentityHandle`].

use wasm_bindgen::prelude::*;

use crate::build::{author_sign, finalize_recipe};
use crate::codec::{decode_file, encode_file, from_canonical_cbor};
use crate::identity::key_fingerprint;
use crate::model::ids::DocId;
use crate::model::payload::{BundlePayload, Kind};
use crate::model::primitives::{Rational, Timestamp};
use crate::model::recipe::Recipe;
use crate::profile::{sign_profile, verify_profile};
use crate::render::scale_recipe;
use crate::sign::{SecretKeySet, keyset_from_passphrase};
use crate::verify::verify;

/// Converts any displayable error into a JS exception value.
fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Installs a panic hook that surfaces Rust panics in the browser console.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Returns the format version this build implements (e.g. `65536` = v1.0.0).
#[wasm_bindgen(js_name = crispFormatVersion)]
#[must_use]
pub fn crisp_format_version() -> u32 {
    crate::FORMAT_VERSION as u32
}

/// Returns the lowest bundle `min_v` this build can open.
#[wasm_bindgen(js_name = minReaderVersion)]
#[must_use]
pub fn min_reader_version() -> u32 {
    crate::MIN_READER_VERSION as u32
}

/// Returns the `crisp-core` crate version string.
#[wasm_bindgen(js_name = coreVersion)]
#[must_use]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Verifies a `.crisp` file and returns its compliance report.
///
/// `platform_keys` is a (possibly empty) JS array of platform public-key-set
/// objects; pass an empty array for offline structural verification.
///
/// # Errors
///
/// Returns a JS exception if `platform_keys` cannot be deserialized or the
/// report cannot be serialized.
#[wasm_bindgen(js_name = verifyBundle)]
pub fn verify_bundle(
    bytes: &[u8],
    platform_keys: JsValue,
    verified_at: f64,
) -> Result<JsValue, JsValue> {
    let platform_keys: Vec<crate::model::meta::PublicKeySet> =
        if platform_keys.is_null() || platform_keys.is_undefined() {
            Vec::new()
        } else {
            serde_wasm_bindgen::from_value(platform_keys).map_err(js_error)?
        };
    let report = verify(bytes, &platform_keys, Timestamp(verified_at as u64));
    serde_wasm_bindgen::to_value(&report).map_err(js_error)
}

/// Decodes a `.crisp` file into its bundle payload (a render model).
///
/// # Errors
///
/// Returns a JS exception if the file is malformed or off-schema.
#[wasm_bindgen(js_name = parseBundle)]
pub fn parse_bundle(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let envelope = decode_file(bytes).map_err(js_error)?;
    let payload: BundlePayload =
        from_canonical_cbor(&envelope.signed_bundle.payload).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&payload).map_err(js_error)
}

/// Scales a recipe by the rational factor `numerator / denominator`.
///
/// # Errors
///
/// Returns a JS exception if the recipe cannot be deserialized or a scaled
/// amount is out of range.
#[wasm_bindgen(js_name = scaleRecipe)]
pub fn scale_recipe_js(
    recipe: JsValue,
    numerator: i64,
    denominator: u32,
) -> Result<JsValue, JsValue> {
    let recipe: Recipe = serde_wasm_bindgen::from_value(recipe).map_err(js_error)?;
    let scaled = scale_recipe(&recipe, Rational::new(numerator, u64::from(denominator)))
        .map_err(js_error)?;
    serde_wasm_bindgen::to_value(&scaled).map_err(js_error)
}

/// An opaque handle to a derived author identity.
///
/// It holds live secret key material inside WASM linear memory; only the
/// public parts are ever exposed to JavaScript.
#[wasm_bindgen]
pub struct IdentityHandle {
    secret: SecretKeySet,
}

#[wasm_bindgen]
impl IdentityHandle {
    /// The `usr…` identity this key owns.
    ///
    /// # Errors
    ///
    /// Returns a JS exception if id derivation fails.
    #[wasm_bindgen(js_name = userId)]
    pub fn user_id(&self) -> Result<String, JsValue> {
        Ok(self.secret.user_id().map_err(js_error)?.0)
    }

    /// The public key set, as a plain JS object.
    ///
    /// # Errors
    ///
    /// Returns a JS exception if serialization fails.
    #[wasm_bindgen(js_name = publicKeyset)]
    pub fn public_keyset(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.secret.public_keyset()).map_err(js_error)
    }

    /// Signs an arbitrary message with the composite key.
    ///
    /// Used for the login-challenge handshake, where the server hands the
    /// client a nonce to sign. The bundle-signing path is [`Self::sign_bundle`].
    ///
    /// # Errors
    ///
    /// Returns a JS exception if signing fails.
    #[wasm_bindgen(js_name = signMessage)]
    pub fn sign_message(&self, message: &[u8]) -> Result<Vec<u8>, JsValue> {
        self.secret.sign(message).map_err(js_error)
    }

    /// Builds a self-signed account profile and returns its `signed-profile`
    /// CBOR bytes — used to register this identity with a server.
    ///
    /// # Errors
    ///
    /// Returns a JS exception if a field is out of range or signing fails.
    #[wasm_bindgen(js_name = signProfile)]
    pub fn sign_profile(
        &self,
        username: &str,
        display_name: &str,
        signed_at: f64,
    ) -> Result<Vec<u8>, JsValue> {
        sign_profile(
            &self.secret,
            username,
            display_name,
            Timestamp(signed_at as u64),
        )
        .map_err(js_error)
    }

    /// Author-signs an already-finalized bundle payload and returns the
    /// `.crisp` file bytes.
    ///
    /// # Errors
    ///
    /// Returns a JS exception if the payload cannot be deserialized or signing
    /// fails.
    #[wasm_bindgen(js_name = signBundle)]
    pub fn sign_bundle(&self, payload: JsValue, signed_at: f64) -> Result<Vec<u8>, JsValue> {
        let payload: BundlePayload = serde_wasm_bindgen::from_value(payload).map_err(js_error)?;
        let envelope =
            author_sign(&payload, &self.secret, Timestamp(signed_at as u64)).map_err(js_error)?;
        encode_file(&envelope).map_err(js_error)
    }

    /// Builds a complete `.crisp` file from a draft payload: every recipe is
    /// finalized (its identity is stamped with this author's fingerprint and
    /// its `id`/`revision` are derived), the document id is set, and the
    /// bundle is author-signed.
    ///
    /// This is the authoring entry point — the draft payload may leave each
    /// recipe's `id`, `revision`, and `identity.author_fpr` empty.
    ///
    /// # Errors
    ///
    /// Returns a JS exception if the draft cannot be deserialized, an identity
    /// cannot be derived, or signing fails.
    #[wasm_bindgen(js_name = buildBundle)]
    pub fn build_bundle(&self, payload: JsValue, signed_at: f64) -> Result<Vec<u8>, JsValue> {
        let mut payload: BundlePayload =
            serde_wasm_bindgen::from_value(payload).map_err(js_error)?;
        let fpr = key_fingerprint(&self.secret.public_keyset()).map_err(js_error)?;

        let mut finalized = Vec::with_capacity(payload.recipes.len());
        for recipe in payload.recipes.drain(..) {
            finalized.push(finalize_recipe(recipe, fpr).map_err(js_error)?);
        }
        payload.recipes = finalized;

        if payload.kind == Kind::Recipe
            && let Some(first) = payload.recipes.first()
        {
            payload.doc_id = DocId(first.id.0.clone());
        }

        let envelope =
            author_sign(&payload, &self.secret, Timestamp(signed_at as u64)).map_err(js_error)?;
        encode_file(&envelope).map_err(js_error)
    }
}

/// Derives an author identity from an account username and passphrase.
///
/// # Errors
///
/// Returns a JS exception if either credential is empty or derivation fails.
#[wasm_bindgen(js_name = deriveIdentity)]
pub fn derive_identity(username: &str, passphrase: &str) -> Result<IdentityHandle, JsValue> {
    let secret = keyset_from_passphrase(username, passphrase).map_err(js_error)?;
    Ok(IdentityHandle { secret })
}

/// Verifies a `signed-profile` and returns the `account-profile` it carries.
///
/// The signature is checked against the keyset inside the profile, so the
/// returned username and display name are self-asserted by that key.
///
/// # Errors
///
/// Returns a JS exception if the bytes are off-schema or the signature is
/// invalid.
#[wasm_bindgen(js_name = verifyProfile)]
pub fn verify_profile_js(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let profile = verify_profile(bytes).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&profile).map_err(js_error)
}
