//! Composite ML-DSA signatures and passphrase-derived keys (spec §8).
//!
//! Crisp signs with **Composite ML-DSA**, parameter set `MLDSA65-Ed25519`: a
//! true hybrid of the post-quantum ML-DSA-65 (FIPS 204) and the classical
//! Ed25519. A signature verifies only if *both* components verify, so a forger
//! must break both primitives — and the bundle stays secure even if one is
//! later broken, including by a quantum adversary.
//!
//! The composite construction is modelled on the IETF LAMPS Composite ML-DSA
//! draft. Crisp is a closed ecosystem — this crate is the single shared
//! implementation for client and server — so the construction is fixed by
//! [`composite::COMPOSITE_CONSTRUCTION`] rather than pinned to external
//! interop: each component independently signs the message under a fixed
//! domain-separating prefix, and the two component signatures are
//! concatenated. The signed message itself is domain-separated by
//! [`message::signing_message`].

pub mod composite;
pub mod keys;
pub mod message;

pub use composite::{COMPOSITE_SIG_LEN, ED25519_SIG_LEN, ML_DSA_65_SIG_LEN, verify_composite};
pub use keys::{SecretKeySet, derive_keyset, keyset_from_passphrase};
pub use message::signing_message;
