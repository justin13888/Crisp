//! Composite ML-DSA `id-MLDSA65-Ed25519-SHA512` sign and verify.
//!
//! This is the construction of `draft-ietf-lamps-pq-composite-sigs-19`
//! (§2.2, §3.2, §3.3), pinned here and checked against that draft's
//! Appendix E test vector:
//!
//! ```text
//! M'       = Prefix || Label || len(ctx) || ctx || SHA-512(M)
//! mldsaSig = ML-DSA-65.Sign(mldsaSK, M', ctx = Label)    (hedged)
//! tradSig  = Ed25519.Sign(edSK, M')
//! s        = mldsaSig || tradSig                          (3373 bytes)
//! ```
//!
//! Crisp passes an empty application context: every message Crisp signs is
//! already domain-separated by a `crisp/…` string inside `M` (see
//! [`crate::sign::message`]).

use ed25519_dalek::{Signature as Ed25519Signature, Signer, VerifyingKey as Ed25519VerifyingKey};
use ml_dsa::{
    EncodedSignature, EncodedVerifyingKey, MlDsa65, Signature as MlDsaSignature,
    VerifyingKey as MlDsaVerifyingKey,
};
use sha2::{Digest, Sha512};

use crate::error::{CrispError, Result};
use crate::model::meta::PublicKeySet;
use crate::sign::keys::SecretKeySet;

/// The composite signature prefix: ASCII `"CompositeAlgorithmSignatures2025"`.
pub const PREFIX: &[u8] = b"CompositeAlgorithmSignatures2025";
/// The signature label of `id-MLDSA65-Ed25519-SHA512`. It is also the
/// context string passed to the ML-DSA component.
pub const LABEL: &[u8] = b"COMPSIG-MLDSA65-Ed25519-SHA512";
/// The application context Crisp signs under (empty; see the module docs).
const CTX: &[u8] = b"";

/// The length of an ML-DSA-65 signature, in bytes.
pub const ML_DSA_65_SIG_LEN: usize = 3309;
/// The length of an Ed25519 signature, in bytes.
pub const ED25519_SIG_LEN: usize = 64;
/// The length of a composite signature, in bytes.
pub const COMPOSITE_SIG_LEN: usize = ML_DSA_65_SIG_LEN + ED25519_SIG_LEN;

/// Builds the message representative `M'` both components sign.
fn representative(message: &[u8], ctx: &[u8]) -> Result<Vec<u8>> {
    let ctx_len = u8::try_from(ctx.len())
        .map_err(|_| CrispError::crypto("composite context exceeds 255 bytes"))?;
    let digest = Sha512::digest(message);
    let mut out = Vec::with_capacity(PREFIX.len() + LABEL.len() + 1 + ctx.len() + digest.len());
    out.extend_from_slice(PREFIX);
    out.extend_from_slice(LABEL);
    out.push(ctx_len);
    out.extend_from_slice(ctx);
    out.extend_from_slice(&digest);
    Ok(out)
}

impl SecretKeySet {
    /// Signs `message` with the composite key, using hedged (randomized)
    /// ML-DSA signing as FIPS 204 recommends.
    ///
    /// # Errors
    ///
    /// Returns [`CrispError::Crypto`] if the system RNG fails or ML-DSA
    /// signing fails.
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>> {
        let m_prime = representative(message, CTX)?;
        let ml_dsa_sig = self
            .ml_dsa
            .expanded_key()
            .sign_randomized(&m_prime, LABEL, &mut getrandom::SysRng)
            .map_err(|e| CrispError::crypto(format!("ML-DSA-65 signing failed: {e}")))?;
        Ok(self.assemble(&ml_dsa_sig, &m_prime))
    }

    /// Signs `message` with deterministic ML-DSA signing. Only for
    /// reproducible artifacts (the compliance suite and golden vectors); live
    /// signing uses [`Self::sign`].
    #[cfg(test)]
    pub(crate) fn sign_deterministic(&self, message: &[u8]) -> Result<Vec<u8>> {
        let m_prime = representative(message, CTX)?;
        let ml_dsa_sig = self
            .ml_dsa
            .expanded_key()
            .sign_deterministic(&m_prime, LABEL)
            .map_err(|e| CrispError::crypto(format!("ML-DSA-65 signing failed: {e}")))?;
        Ok(self.assemble(&ml_dsa_sig, &m_prime))
    }

    /// Concatenates the ML-DSA signature with a fresh Ed25519 signature.
    fn assemble(&self, ml_dsa_sig: &MlDsaSignature<MlDsa65>, m_prime: &[u8]) -> Vec<u8> {
        let ed_sig = self.ed25519.sign(m_prime);
        let mut out = Vec::with_capacity(COMPOSITE_SIG_LEN);
        out.extend_from_slice(ml_dsa_sig.encode().as_slice());
        out.extend_from_slice(&ed_sig.to_bytes());
        out
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
    verify_with_context(keyset, message, CTX, signature)
}

/// [`verify_composite`] with an explicit application context.
fn verify_with_context(
    keyset: &PublicKeySet,
    message: &[u8],
    ctx: &[u8],
    signature: &[u8],
) -> Result<()> {
    if signature.len() != COMPOSITE_SIG_LEN {
        return Err(CrispError::crypto(format!(
            "composite signature must be {COMPOSITE_SIG_LEN} bytes, got {}",
            signature.len()
        )));
    }
    let m_prime = representative(message, ctx)?;
    let (ml_dsa_part, ed_part) = signature.split_at(ML_DSA_65_SIG_LEN);

    let ml_dsa_vk_bytes = EncodedVerifyingKey::<MlDsa65>::try_from(keyset.ml_dsa_65.as_slice())
        .map_err(|_| CrispError::crypto("ML-DSA-65 public key has the wrong length"))?;
    let ml_dsa_vk = MlDsaVerifyingKey::<MlDsa65>::decode(&ml_dsa_vk_bytes);
    let ml_dsa_sig_bytes = EncodedSignature::<MlDsa65>::try_from(ml_dsa_part)
        .map_err(|_| CrispError::crypto("ML-DSA-65 signature has the wrong length"))?;
    let ml_dsa_sig = MlDsaSignature::<MlDsa65>::decode(&ml_dsa_sig_bytes)
        .ok_or_else(|| CrispError::crypto("malformed ML-DSA-65 signature"))?;
    let ml_dsa_ok = ml_dsa_vk.verify_with_context(&m_prime, LABEL, &ml_dsa_sig);

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
    let ed_ok = ed_vk.verify_strict(&m_prime, &ed_sig).is_ok();

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
    use crate::sign::keys::SecretKeySet;

    /// The draft's Appendix E vector for `id-MLDSA65-Ed25519-SHA512`.
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Vector {
        m: String,
        ctx: String,
        pk: String,
        sk: String,
        s: String,
        s_with_context: String,
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
            .collect()
    }

    fn vector() -> Vector {
        serde_json::from_str(include_str!("composite_vector.json")).expect("vector json")
    }

    fn vector_keyset(v: &Vector) -> PublicKeySet {
        let pk = hex(&v.pk);
        PublicKeySet {
            ml_dsa_65: pk[..1952].to_vec(),
            ed25519: pk[1952..].to_vec(),
        }
    }

    #[test]
    fn verifies_the_draft_test_vector() {
        let v = vector();
        let keyset = vector_keyset(&v);
        verify_composite(&keyset, v.m.as_bytes(), &hex(&v.s)).expect("empty-ctx vector");
        verify_with_context(
            &keyset,
            v.m.as_bytes(),
            v.ctx.as_bytes(),
            &hex(&v.s_with_context),
        )
        .expect("ctx vector");
        // The context is bound: the ctx signature fails without it.
        assert!(verify_composite(&keyset, v.m.as_bytes(), &hex(&v.s_with_context)).is_err());
    }

    #[test]
    fn the_draft_private_key_reproduces_its_public_key_and_signs_interoperably() {
        let v = vector();
        let sk = hex(&v.sk);
        let secret = SecretKeySet::from_components(
            sk[..32].try_into().expect("ml-dsa seed"),
            sk[32..].try_into().expect("ed25519 key"),
        );
        assert_eq!(secret.public_keyset(), vector_keyset(&v));
        let signature = secret.sign(v.m.as_bytes()).expect("sign");
        verify_composite(&vector_keyset(&v), v.m.as_bytes(), &signature).expect("verify");
    }

    #[test]
    fn sign_then_verify_round_trips() {
        let secret = SecretKeySet::from_seed(&[1u8; 32]);
        let message = b"a Crisp signing message";
        let signature = secret.sign(message).expect("sign");
        assert_eq!(signature.len(), COMPOSITE_SIG_LEN);
        verify_composite(&secret.public_keyset(), message, &signature).expect("verify");
    }

    #[test]
    fn hedged_signing_is_randomized_and_deterministic_signing_is_not() {
        let secret = SecretKeySet::from_seed(&[2u8; 32]);
        assert_ne!(
            secret.sign(b"message").expect("sign"),
            secret.sign(b"message").expect("sign")
        );
        assert_eq!(
            secret.sign_deterministic(b"message").expect("sign"),
            secret.sign_deterministic(b"message").expect("sign")
        );
    }

    #[test]
    fn a_tampered_message_fails_verification() {
        let secret = SecretKeySet::from_seed(&[3u8; 32]);
        let signature = secret.sign(b"original").expect("sign");
        assert!(verify_composite(&secret.public_keyset(), b"tampered", &signature).is_err());
    }

    #[test]
    fn another_keys_signature_fails_verification() {
        let signer = SecretKeySet::from_seed(&[4u8; 32]);
        let other = SecretKeySet::from_seed(&[5u8; 32]);
        let signature = signer.sign(b"message").expect("sign");
        assert!(verify_composite(&other.public_keyset(), b"message", &signature).is_err());
    }

    #[test]
    fn a_lone_component_does_not_verify_as_composite() {
        let secret = SecretKeySet::from_seed(&[6u8; 32]);
        let keyset = secret.public_keyset();
        let signature = secret.sign(b"message").expect("sign");

        let ed_only = &signature[ML_DSA_65_SIG_LEN..];
        assert!(verify_composite(&keyset, b"message", ed_only).is_err());

        let mut half_forged = vec![0u8; ML_DSA_65_SIG_LEN];
        half_forged.extend_from_slice(&signature[ML_DSA_65_SIG_LEN..]);
        assert!(verify_composite(&keyset, b"message", &half_forged).is_err());
    }
}
