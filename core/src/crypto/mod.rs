//! Cryptographic primitives: hashing and bech32m identifiers.
//!
//! Crisp uses SHA-256 everywhere (spec decision D4) and encodes every
//! identifier as bech32m over a versioned 32-byte digest (spec §7). These are
//! the building blocks the `identity` and `sign` modules compose into ID
//! derivation and Composite ML-DSA signatures.

pub mod bech32id;
pub mod hash;

pub use bech32id::{Hrp, decode_id, encode_id};
pub use hash::sha256;
