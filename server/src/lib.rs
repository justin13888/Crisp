//! # crisp-server
//!
//! The Crisp backend. It is a thin application layer over [`crisp_core`]: the
//! HTTP surface, a PostgreSQL store, crypto-bound accounts, bundle publication
//! with server endorsement, full-text search, and version negotiation. Every
//! format detail — canonical CBOR, identity derivation, signatures, the A–E
//! verification pipeline — lives in `crisp-core` and is *never* re-implemented
//! here, so the server and client cannot drift in format semantics.
//!
//! ## Module map
//!
//! - [`config`] — process configuration loaded from the environment.
//! - [`db`] — SeaORM entities and the schema migration.
//! - [`storage`] — the mockable repository trait and its PostgreSQL backing.
//! - [`index`] — pure extraction of search metadata from a bundle payload.
//! - [`platform`] — the server's endorsement signing key.
//! - [`error`] — the API error type and its HTTP projection.

pub mod config;
pub mod db;
pub mod error;
pub mod index;
pub mod platform;
pub mod storage;
pub mod util;

pub use error::{ApiError, ApiResult};
