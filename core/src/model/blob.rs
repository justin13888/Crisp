//! Content-addressed media blobs (`crisp-v1.cddl`: `blob`).

use serde::{Deserialize, Serialize};

use crate::model::primitives::OpenCode;

/// An inline image. Every blob is content-addressed: its `blb…` id is the
/// bech32m encoding of `SHA-256(bytes)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blob {
    /// The image's media type. Anything outside the allowlist decodes as
    /// [`OpenCode::Other`] and fails check C1 with `E.BLOB.MEDIATYPE`.
    pub media_type: OpenCode<MediaType>,
    /// The byte length of `bytes`; must match it exactly.
    pub size: u64,
    /// Image width in pixels, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u64>,
    /// Image height in pixels, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
    /// The raw image bytes.
    #[serde(with = "serde_bytes")]
    pub bytes: Vec<u8>,
}

/// The allowlisted raster image formats (`crisp-v1.cddl`: `blob.media_type`).
///
/// Only raster formats are permitted — no SVG, video, audio, or fonts — to
/// keep the viewer attack surface minimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaType {
    /// PNG (`image/png`).
    #[serde(rename = "image/png")]
    Png,
    /// JPEG (`image/jpeg`).
    #[serde(rename = "image/jpeg")]
    Jpeg,
    /// WebP (`image/webp`).
    #[serde(rename = "image/webp")]
    Webp,
    /// AVIF (`image/avif`).
    #[serde(rename = "image/avif")]
    Avif,
}
