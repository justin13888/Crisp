//! # crisp-core
//!
//! The engine for the **Crisp recipe bundle format** — the single, shared
//! implementation of every format detail described by the normative spec in
//! `spec/crisp-v1.cddl` and `spec/README.md`.
//!
//! This crate is the source of truth for format behaviour across the whole
//! product. It is compiled twice:
//!
//! - natively (`rlib`) and linked into the `crisp-server` backend, and
//! - to WebAssembly (`cdylib`, via `wasm-pack`) and consumed by the web client.
//!
//! Writing the codec, cryptography, identity derivation, and verification
//! pipeline exactly once eliminates client/server drift in format semantics.
//!
//! ## Module map
//!
//! The modules below are introduced stage by stage; this skeleton intentionally
//! starts empty so each stage lands as a self-contained, reviewable commit:
//!
//! - `error`, `limits` — stable error codes and the spec §12 hard limits.
//! - `cbor` — deterministic canonical CBOR (RFC 8949 §4.2) encode/decode.
//! - `crypto` — SHA-256, bech32m IDs, and the Argon2id key-derivation function.
//! - `model` — the Crisp data model, mirroring `crisp-v1.cddl`.
//! - `codec` — model <-> canonical CBOR conversion.
//! - `identity` — domain-separated ID derivation and passphrase-based keypairs.
//! - `sign` — Composite ML-DSA (`MLDSA65-Ed25519`) signing and verification.
//! - `profile` — self-signed account profiles (display name bound to a key).
//! - `verify` — the staged A–E verification pipeline and compliance report.
//! - `build`, `render` — bundle authoring and client-side render helpers.
//! - `wasm` — the minimal `wasm-bindgen` API (behind the `wasm` feature).

// `deny` (not `forbid`) so the Stage 10 `wasm` module can locally opt in to the
// `unsafe` that `wasm-bindgen`'s generated glue requires.
#![deny(unsafe_code)]
#![deny(missing_docs)]

/// The format version this build of `core` implements (v1.0.0).
pub const FORMAT_VERSION: u64 = 0x01_0000;
/// The lowest bundle `min_v` this build of `core` can open.
pub const MIN_READER_VERSION: u64 = 0x01_0000;

pub mod build;
pub mod cbor;
pub mod codec;
pub mod crypto;
pub mod error;
pub mod identity;
pub mod limits;
pub mod model;
pub mod profile;
pub mod render;
pub mod sign;
pub mod verify;

#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(test)]
mod fixtures;

pub use error::{CrispError, ErrorCode, Phase, Result};
