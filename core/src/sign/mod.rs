//! Composite ML-DSA signatures and identity keys (spec §8).
//!
//! Crisp signs with **Composite ML-DSA**, `id-MLDSA65-Ed25519-SHA512`
//! (`draft-ietf-lamps-pq-composite-sigs-19`): a hybrid of the post-quantum
//! ML-DSA-65 (FIPS 204) and the classical Ed25519. A signature verifies only
//! if *both* components verify, so a forger must break both primitives.
//!
//! Every message Crisp signs is domain-separated before it reaches the
//! composite scheme — see [`message`].

pub mod composite;
pub mod keys;
pub mod message;

pub use composite::{COMPOSITE_SIG_LEN, ED25519_SIG_LEN, ML_DSA_65_SIG_LEN, verify_composite};
pub use keys::{IdentitySeed, SEED_LEN, SecretKeySet};
pub use message::{login_message, signing_message};
