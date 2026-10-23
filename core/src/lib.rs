//! # crisp-core
//!
//! The engine for the **Crisp recipe bundle format** — the single, shared
//! implementation of every format detail described by the normative spec in
//! `spec/crisp-v1.cddl` and `spec/README.md`.
//!
//! This crate is the source of truth for format behaviour across the whole
//! product. It is compiled twice:
//!
//! - natively, linked into the `crisp-server` backend, and
//! - to WebAssembly, wrapped by the `crisp-wasm` crate for the web client.
//!
//! Writing the codec, cryptography, identity derivation, and verification
//! pipeline exactly once eliminates client/server drift in format semantics.
//!
//! ## Module map
//!
//! - `error`, `limits` — stable error codes and the spec §12 hard limits.
//! - `cbor`, `codec` — deterministic canonical CBOR and model conversion.
//! - `crypto` — SHA-256 and bech32m identifiers.
//! - `model` — the Crisp data model, mirroring `crisp-v1.cddl`.
//! - `identity` — domain-separated id, revision, and fingerprint derivation.
//! - `sign` — Composite ML-DSA `id-MLDSA65-Ed25519-SHA512` and identity keys.
//! - `keystore` — passphrase-wrapped identity key files (`.crispkey`).
//! - `profile` — self-signed account profiles (display name bound to a key).
//! - `verify` — the staged A–E verification pipeline and compliance report.
//! - `author` — sealing drafts into signed files; revisions, forks, endorsing.
//! - `render` — scaling, units, formatting, parsing, and the recipe view.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// The format version this build of `core` implements (v1.0.0).
pub const FORMAT_VERSION: u64 = 0x01_0000;
/// The lowest bundle `min_v` this build of `core` can open.
pub const MIN_READER_VERSION: u64 = 0x01_0000;

pub mod author;
pub mod cbor;
pub mod codec;
pub mod crypto;
pub mod error;
pub mod identity;
pub mod keystore;
pub mod limits;
pub mod model;
pub mod profile;
pub mod render;
pub mod sign;
pub mod verify;

#[cfg(test)]
mod fixtures;

pub use error::{CrispError, ErrorCode, Phase, Result};
