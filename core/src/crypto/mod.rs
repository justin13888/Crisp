//! Cryptographic primitives: hashing, bech32m identifiers, and the
//! passphrase key-derivation function.
//!
//! Crisp uses SHA-256 everywhere (spec decision D4), encodes every identifier
//! as bech32m over a versioned 32-byte digest (spec §7), and derives an
//! author's identity deterministically from a username and passphrase. These
//! are the building blocks the `identity` and `sign` modules compose into ID
//! derivation and Composite ML-DSA signatures.

pub mod bech32id;
pub mod hash;
pub mod kdf;

pub use bech32id::{Hrp, decode_id, encode_id};
pub use hash::sha256;
pub use kdf::derive_seed;
