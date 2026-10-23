//! Construction of the domain-separated message that signatures cover.

use ciborium::value::Value;

use crate::cbor::encode_canonical;
use crate::error::Result;

/// The domain separator for the Crisp signing message.
const SIGNATURE_DOMAIN: &str = "crisp/signature/1";
/// The domain separator for a login-challenge response.
const LOGIN_DOMAIN: &str = "crisp/login/1";

/// Builds the message `M` that a signature is computed over (spec §8).
///
/// `M = canonical_cbor(["crisp/signature/1", protected_header, sig_protected,
/// external_aad])`, where `protected_header` and `sig_protected` are the
/// already-CBOR-encoded byte strings of those headers.
///
/// For an author signature `external_aad` is empty; for a server endorsement
/// it is `SHA-256(canonical_cbor(author_signature))`, which binds the
/// endorsement to that exact author-signed bundle.
///
/// # Errors
///
/// Returns an error if canonical CBOR encoding fails.
pub fn signing_message(
    protected_header: &[u8],
    sig_protected: &[u8],
    external_aad: &[u8],
) -> Result<Vec<u8>> {
    encode_canonical(&Value::Array(vec![
        Value::Text(SIGNATURE_DOMAIN.to_owned()),
        Value::Bytes(protected_header.to_vec()),
        Value::Bytes(sig_protected.to_vec()),
        Value::Bytes(external_aad.to_vec()),
    ]))
}

/// Builds the message a client signs to answer a server login challenge.
///
/// `M = canonical_cbor(["crisp/login/1", audience, nonce])`, where `audience`
/// is the `usr…` id of the server's platform key. Binding the audience means
/// a challenge relayed from one server to another yields a signature the
/// second server rejects.
///
/// # Errors
///
/// Returns an error if canonical CBOR encoding fails.
pub fn login_message(audience: &str, nonce: &[u8]) -> Result<Vec<u8>> {
    encode_canonical(&Value::Array(vec![
        Value::Text(LOGIN_DOMAIN.to_owned()),
        Value::Text(audience.to_owned()),
        Value::Bytes(nonce.to_vec()),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_message_is_deterministic() {
        let a = signing_message(b"\xa0", b"\xa0", b"").expect("build");
        let b = signing_message(b"\xa0", b"\xa0", b"").expect("build");
        assert_eq!(a, b);
    }

    #[test]
    fn login_messages_are_bound_to_their_audience() {
        let a = login_message("usr1a", &[1u8; 32]).expect("build");
        let b = login_message("usr1b", &[1u8; 32]).expect("build");
        assert_ne!(a, b);
        assert_ne!(a, signing_message(b"", b"", &[1u8; 32]).expect("build"));
    }

    #[test]
    fn external_aad_changes_the_message() {
        let author = signing_message(b"\xa0", b"\xa0", b"").expect("build");
        let endorsement = signing_message(b"\xa0", b"\xa0", &[0u8; 32]).expect("build");
        assert_ne!(author, endorsement);
    }
}
