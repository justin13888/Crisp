//! Endorsement revocation for JavaScript (spec §8.8): accepting a server's
//! signed revocation list against the keys pinned for it, and verifying a
//! bundle against the revocations held for that server.
//!
//! The client fetches the list with `CrispServer.revocations`, passes it to
//! `acceptRevocations` with its pin and what it already holds, stores the
//! result, and verifies with `Bundle.verifyWithRevocations`.

use crisp_core::model::primitives::Timestamp;
use crisp_core::verify::verify_with_revocations;
use crisp_core::verify::{PlatformKey, Revocation, accept_revocations as accept};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use crate::api::types;
use crate::boundary::{core_error, decode_hex, failure, from_js, timestamp, to_js};
use crate::format::Bundle;

/// What the client needs from `GET /revocations`.
#[derive(Debug, Serialize, Deserialize, Tsify)]
pub struct RevocationsView {
    /// The revoked bundles, in the order the server signed them. Hold them
    /// only as `acceptRevocations` decides.
    pub revocations: Vec<Revocation>,
    /// The hex signatures over `revocations`, for `acceptRevocations`.
    pub signatures: Vec<String>,
}

/// Converts the generated `GET /revocations` body.
///
/// # Errors
///
/// Fails when a content hash is not hex or a time is negative: the list as
/// signed cannot then be rebuilt, so none of it can be trusted.
pub(crate) fn revocations_view(body: types::Revocations) -> Result<RevocationsView, JsValue> {
    let malformed = || failure("the server's revocation list is malformed", None, None);
    let revocations = body
        .revocations
        .iter()
        .map(|r| {
            Ok(Revocation {
                content_hash: decode_hex(&r.content_hash).ok_or_else(malformed)?,
                revoked_at: Timestamp(u64::try_from(r.revoked_at).map_err(|_| malformed())?),
            })
        })
        .collect::<Result<_, JsValue>>()?;
    Ok(RevocationsView {
        revocations,
        signatures: body.signatures.into_iter().map(|s| s.sig).collect(),
    })
}

/// The revocations to hold for a server: `held` plus the ones `offered`
/// adds, accepted only when a key `pinned` as current for that server signed
/// the offered list (spec §8.8). A signature that is not hex vouches for
/// nothing.
///
/// # Errors
///
/// Throws when the arguments are malformed or no current pinned key signed
/// the list; keep holding `held` then.
#[wasm_bindgen(js_name = acceptRevocations, unchecked_return_type = "Revocation[]")]
pub fn accept_revocations(
    #[wasm_bindgen(unchecked_param_type = "PlatformKey[]")] pinned: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Revocation[]")] held: JsValue,
    #[wasm_bindgen(unchecked_param_type = "RevocationsView")] offered: JsValue,
) -> Result<JsValue, JsValue> {
    let pinned: Vec<PlatformKey> = from_js(pinned, "pinned keys")?;
    let held: Vec<Revocation> = from_js(held, "held revocations")?;
    let offered: RevocationsView = from_js(offered, "revocation list")?;
    let signatures: Vec<Vec<u8>> = (offered.signatures.iter())
        .filter_map(|sig| decode_hex(sig))
        .collect();
    let merged =
        accept(&pinned, &held, &offered.revocations, &signatures).map_err(|e| core_error(&e))?;
    to_js(&merged)
}

#[wasm_bindgen]
impl Bundle {
    /// Verifies the file against `platformKeys` and the `revocations` held
    /// for the same server; a revoked endorsement fails E2 with
    /// `E.SIG.REVOKED` and trust falls to `author-signed`.
    ///
    /// # Errors
    ///
    /// Throws if the arguments are malformed.
    #[wasm_bindgen(js_name = verifyWithRevocations, unchecked_return_type = "ComplianceReport")]
    pub fn verify_with_revocations(
        &self,
        #[wasm_bindgen(js_name = platformKeys, unchecked_param_type = "PlatformKey[]")]
        platform_keys: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Revocation[]")] revocations: JsValue,
        now: f64,
    ) -> Result<JsValue, JsValue> {
        let keys: Vec<PlatformKey> = from_js(platform_keys, "platform keys")?;
        let revoked: Vec<Revocation> = from_js(revocations, "revocations")?;
        let report = verify_with_revocations(&self.bytes(), &keys, &revoked, timestamp(now)?);
        to_js(&report)
    }
}

#[cfg(test)]
mod tests {
    use super::{RevocationsView, revocations_view};
    use crate::api::types;

    fn body(content_hash: &str, revoked_at: i64) -> types::Revocations {
        serde_json::from_value(serde_json::json!({
            "revocations": [{ "content_hash": content_hash, "revoked_at": revoked_at }],
            "signatures": [{ "usr_id": "usr1platform", "sig": "abcd" }],
        }))
        .expect("a revocations body")
    }

    #[test]
    fn a_list_decodes_with_its_signatures() {
        let view: RevocationsView = revocations_view(body(&"0a".repeat(32), 7)).expect("view");
        assert_eq!(view.revocations[0].content_hash, vec![10; 32]);
        assert_eq!(view.revocations[0].revoked_at.0, 7);
        assert_eq!(view.signatures, ["abcd"]);
    }
}
