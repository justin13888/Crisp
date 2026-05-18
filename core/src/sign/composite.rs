//! The Composite ML-DSA (`MLDSA65-Ed25519`) sign and verify operations.

use ed25519_dalek::{Signature as Ed25519Signature, Signer, VerifyingKey as Ed25519VerifyingKey};
use ml_dsa::{
    EncodedSignature, EncodedVerifyingKey, MlDsa65, Signature as MlDsaSignature,
    VerifyingKey as MlDsaVerifyingKey,
};

use crate::error::{CrispError, Result};
use crate::model::meta::PublicKeySet;
use crate::sign::keys::SecretKeySet;

/// An identifier for the composite construction this module implements.
///
/// It is modelled on the IETF LAMPS Composite ML-DSA draft but fixed here for
/// Crisp's closed client/server ecosystem; bumping it is a breaking change to
/// every signature.
pub const COMPOSITE_CONSTRUCTION: &str = "crisp-composite/MLDSA65-Ed25519/1";

/// The length of an ML-DSA-65 signature, in bytes.
pub const ML_DSA_65_SIG_LEN: usize = 3309;
/// The length of an Ed25519 signature, in bytes.
pub const ED25519_SIG_LEN: usize = 64;
/// The length of a composite signature, in bytes.
pub const COMPOSITE_SIG_LEN: usize = ML_DSA_65_SIG_LEN + ED25519_SIG_LEN;

/// The fixed domain-separating prefix each component signs under, so a
/// component signature made for Crisp cannot be confused with a stand-alone
/// signature of the same primitive.
const COMPONENT_DOMAIN: &[u8] = b"crisp/composite-sig/MLDSA65-Ed25519/1";

/// Builds the domain-prefixed bytes that each component signs.
fn component_message(message: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(COMPONENT_DOMAIN.len() + message.len());
    buf.extend_from_slice(COMPONENT_DOMAIN);
    buf.extend_from_slice(message);
    buf
}

impl SecretKeySet {
    /// Signs `message` with the composite key.
    ///
    /// The result is the ML-DSA-65 signature followed by the Ed25519
    /// signature; both are produced deterministically.
    ///
    /// # Errors
    ///
    /// Returns [`CrispError::Crypto`] if ML-DSA signing fails.
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        let component = component_message(message);
        let ml_dsa_sig = self
            .ml_dsa
            .expanded_key()
            .sign_deterministic(&component, &[])
            .map_err(|e| CrispError::crypto(format!("ML-DSA-65 signing failed: {e}")))?;
        let ed_sig = self.ed25519.sign(&component);

        let mut out = Vec::with_capacity(COMPOSITE_SIG_LEN);
        out.extend_from_slice(ml_dsa_sig.encode().as_slice());
        out.extend_from_slice(&ed_sig.to_bytes());
        Ok(out)
    }
}

/// Verifies a composite signature over `message` against a [`PublicKeySet`].
///
/// Both the ML-DSA-65 and the Ed25519 component must verify; if either fails,
/// the whole signature is rejected.
///
/// # Errors
///
/// Returns [`CrispError::Crypto`] if the signature is the wrong length, a key
/// or component signature is malformed, or verification fails.
pub fn verify_composite(keyset: &PublicKeySet, message: &[u8], signature: &[u8]) -> Result<()> {
    if signature.len() != COMPOSITE_SIG_LEN {
        return Err(CrispError::crypto(format!(
            "composite signature must be {COMPOSITE_SIG_LEN} bytes, got {}",
            signature.len()
        )));
    }
    let component = component_message(message);
    let (ml_dsa_part, ed_part) = signature.split_at(ML_DSA_65_SIG_LEN);

    // ML-DSA-65 component.
    let ml_dsa_vk_bytes = EncodedVerifyingKey::<MlDsa65>::try_from(keyset.ml_dsa_65.as_slice())
        .map_err(|_| CrispError::crypto("ML-DSA-65 public key has the wrong length"))?;
    let ml_dsa_vk = MlDsaVerifyingKey::<MlDsa65>::decode(&ml_dsa_vk_bytes);
    let ml_dsa_sig_bytes = EncodedSignature::<MlDsa65>::try_from(ml_dsa_part)
        .map_err(|_| CrispError::crypto("ML-DSA-65 signature has the wrong length"))?;
    let ml_dsa_sig = MlDsaSignature::<MlDsa65>::decode(&ml_dsa_sig_bytes)
        .ok_or_else(|| CrispError::crypto("malformed ML-DSA-65 signature"))?;
    let ml_dsa_ok = ml_dsa_vk.verify_with_context(&component, &[], &ml_dsa_sig);

    // Ed25519 component.
    let ed_vk_bytes: [u8; 32] = keyset
        .ed25519
        .as_slice()
        .try_into()
        .map_err(|_| CrispError::crypto("Ed25519 public key has the wrong length"))?;
    let ed_vk = Ed25519VerifyingKey::from_bytes(&ed_vk_bytes)
        .map_err(|e| CrispError::crypto(format!("malformed Ed25519 public key: {e}")))?;
    let ed_sig_bytes: [u8; ED25519_SIG_LEN] = ed_part
        .try_into()
        .map_err(|_| CrispError::crypto("Ed25519 signature has the wrong length"))?;
    let ed_sig = Ed25519Signature::from_bytes(&ed_sig_bytes);
    let ed_ok = ed_vk.verify_strict(&component, &ed_sig).is_ok();

    if ml_dsa_ok && ed_ok {
        Ok(())
    } else {
        Err(CrispError::crypto(
            "composite signature verification failed",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sign::keys::derive_keyset;

    #[test]
    fn sign_then_verify_round_trips() {
        let secret = derive_keyset(&[1u8; 32]);
        let message = b"a Crisp signing message";
        let signature = secret.sign(message).expect("sign");
        assert_eq!(signature.len(), COMPOSITE_SIG_LEN);
        verify_composite(&secret.public_keyset(), message, &signature).expect("verify");
    }

    #[test]
    fn signing_is_deterministic() {
        let secret = derive_keyset(&[2u8; 32]);
        let a = secret.sign(b"message").expect("sign");
        let b = secret.sign(b"message").expect("sign");
        assert_eq!(a, b, "both components sign deterministically");
    }

    #[test]
    fn a_tampered_message_fails_verification() {
        let secret = derive_keyset(&[3u8; 32]);
        let signature = secret.sign(b"original").expect("sign");
        assert!(verify_composite(&secret.public_keyset(), b"tampered", &signature).is_err());
    }

    #[test]
    fn another_keys_signature_fails_verification() {
        let signer = derive_keyset(&[4u8; 32]);
        let other = derive_keyset(&[5u8; 32]);
        let signature = signer.sign(b"message").expect("sign");
        assert!(verify_composite(&other.public_keyset(), b"message", &signature).is_err());
    }

    #[test]
    fn a_lone_component_does_not_verify_as_composite() {
        let secret = derive_keyset(&[6u8; 32]);
        let keyset = secret.public_keyset();
        let signature = secret.sign(b"message").expect("sign");

        // The Ed25519 component alone is not even composite-sized.
        let ed_only = &signature[ML_DSA_65_SIG_LEN..];
        assert!(verify_composite(&keyset, b"message", ed_only).is_err());

        // Zeroing the ML-DSA half (keeping a valid Ed25519 half) still fails:
        // a forger must break *both* primitives.
        let mut half_forged = vec![0u8; ML_DSA_65_SIG_LEN];
        half_forged.extend_from_slice(&signature[ML_DSA_65_SIG_LEN..]);
        assert!(verify_composite(&keyset, b"message", &half_forged).is_err());
    }
}
