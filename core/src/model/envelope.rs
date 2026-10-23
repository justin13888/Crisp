//! The COSE-style signed envelope that frames every `.crisp` file
//! (`crisp-v1.cddl`: `crisp-envelope`, `cose-sign`, `signature`).
//!
//! The envelope is the *unsigned* framing; the protected header and payload
//! are byte strings whose decoded contents are what the signatures cover.
//! The array-shaped structures here have hand-written CBOR (de)serialization
//! so their fields stay named while still encoding as CBOR arrays.

use core::fmt;

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};

use crate::model::ids::DocId;
use crate::model::primitives::Timestamp;

/// The container framing version (`crisp-v1.cddl`: `container-version`).
pub const CONTAINER_VERSION: u64 = 1;

/// An advisory, never-signed CBOR header map. Its keys may be integers or
/// strings; conformant readers do not depend on its contents.
pub type HeaderMap = ciborium::value::Value;

/// The outermost framing of a `.crisp` file (`crisp-v1.cddl`:
/// `crisp-envelope`). Encoded as the CBOR array `[container_version,
/// signed_bundle]`.
#[derive(Debug, Clone, PartialEq)]
pub struct CrispEnvelope {
    /// The container framing version; must be [`CONTAINER_VERSION`].
    pub container_version: u64,
    /// The COSE-style signed structure.
    pub signed_bundle: CoseSign,
}

/// A COSE-style signed structure (`crisp-v1.cddl`: `cose-sign`). Encoded as
/// the CBOR array `[protected, unprotected, payload, signatures]`.
#[derive(Debug, Clone, PartialEq)]
pub struct CoseSign {
    /// The protected header, as a CBOR byte string (decodes to
    /// [`ProtectedHeader`]).
    pub protected: Vec<u8>,
    /// The advisory unprotected header.
    pub unprotected: HeaderMap,
    /// The bundle payload, as a CBOR byte string (decodes to
    /// [`crate::model::payload::BundlePayload`]).
    pub payload: Vec<u8>,
    /// The author signatures followed by the single server endorsement.
    pub signatures: Vec<Signature>,
}

/// One signature over the bundle (`crisp-v1.cddl`: `signature`). Encoded as
/// the CBOR array `[protected, unprotected, sig_value]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Signature {
    /// The signature's protected header, as a CBOR byte string (decodes to
    /// [`SigProtected`]).
    pub protected: Vec<u8>,
    /// The advisory unprotected header.
    pub unprotected: HeaderMap,
    /// The Composite ML-DSA signature bytes.
    pub sig_value: Vec<u8>,
}

/// The signed protected header of the envelope (`crisp-v1.cddl`:
/// `protected-header`). Encoded as a CBOR map with the integer keys `1`, `2`,
/// and `3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedHeader {
    /// Key `1` — the Crisp format version.
    pub format_version: u64,
    /// Key `2` — the document id.
    pub doc_id: DocId,
    /// Key `3` — `SHA-256` of the payload (32 bytes).
    pub content_hash: Vec<u8>,
}

/// The signed protected header of one signature (`crisp-v1.cddl`:
/// `sig-protected`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SigProtected {
    /// The signature algorithm; always Composite ML-DSA.
    pub alg: SigAlg,
    /// The signer's role.
    pub role: SigRole,
    /// The signer's key fingerprint (32 bytes); see
    /// [`crate::identity::key_fingerprint`].
    #[serde(with = "serde_bytes")]
    pub kid: Vec<u8>,
    /// When the signature was made.
    pub signed_at: Timestamp,
    /// For a server endorsement only: `SHA-256` of the canonical CBOR array
    /// of every author signature, binding the endorsement to them.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "optional_bytes"
    )]
    pub binds: Option<Vec<u8>>,
}

/// `serde` adapter for an optional byte string.
mod optional_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serializes `Some(bytes)` as a CBOR byte string.
    pub(super) fn serialize<S: Serializer>(
        value: &Option<Vec<u8>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(bytes) => serializer.serialize_bytes(bytes),
            None => serializer.serialize_none(),
        }
    }

    /// Deserializes a CBOR byte string.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<u8>>, D::Error> {
        Ok(Some(
            serde_bytes::ByteBuf::deserialize(deserializer)?.into_vec(),
        ))
    }
}

/// The signature algorithm identifier (`crisp-v1.cddl`: `sig-protected.alg`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SigAlg {
    /// Composite ML-DSA `id-MLDSA65-Ed25519-SHA512` (spec §8).
    #[serde(rename = "MLDSA65-Ed25519")]
    MlDsa65Ed25519,
}

/// The role a signature plays (`crisp-v1.cddl`: `sig-protected.role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SigRole {
    /// An author asserting authorship and integrity.
    Author,
    /// The platform endorsing a published bundle.
    ServerEndorsement,
}

impl Serialize for CrispEnvelope {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(&self.container_version)?;
        seq.serialize_element(&self.signed_bundle)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for CrispEnvelope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (container_version, signed_bundle) = <(u64, CoseSign)>::deserialize(deserializer)?;
        Ok(Self {
            container_version,
            signed_bundle,
        })
    }
}

impl Serialize for CoseSign {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(4))?;
        seq.serialize_element(serde_bytes::Bytes::new(&self.protected))?;
        seq.serialize_element(&self.unprotected)?;
        seq.serialize_element(serde_bytes::Bytes::new(&self.payload))?;
        seq.serialize_element(&self.signatures)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for CoseSign {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (protected, unprotected, payload, signatures) = <(
            serde_bytes::ByteBuf,
            HeaderMap,
            serde_bytes::ByteBuf,
            Vec<Signature>,
        )>::deserialize(deserializer)?;
        Ok(Self {
            protected: protected.into_vec(),
            unprotected,
            payload: payload.into_vec(),
            signatures,
        })
    }
}

impl Serialize for Signature {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(3))?;
        seq.serialize_element(serde_bytes::Bytes::new(&self.protected))?;
        seq.serialize_element(&self.unprotected)?;
        seq.serialize_element(serde_bytes::Bytes::new(&self.sig_value))?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (protected, unprotected, sig_value) =
            <(serde_bytes::ByteBuf, HeaderMap, serde_bytes::ByteBuf)>::deserialize(deserializer)?;
        Ok(Self {
            protected: protected.into_vec(),
            unprotected,
            sig_value: sig_value.into_vec(),
        })
    }
}

impl Serialize for ProtectedHeader {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry(&1u64, &self.format_version)?;
        map.serialize_entry(&2u64, &self.doc_id)?;
        map.serialize_entry(&3u64, serde_bytes::Bytes::new(&self.content_hash))?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for ProtectedHeader {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ProtectedHeaderVisitor;

        impl<'de> Visitor<'de> for ProtectedHeaderVisitor {
            type Value = ProtectedHeader;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("the Crisp protected header (a map with integer keys 1, 2, 3)")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<ProtectedHeader, M::Error> {
                let mut format_version: Option<u64> = None;
                let mut doc_id: Option<DocId> = None;
                let mut content_hash: Option<serde_bytes::ByteBuf> = None;
                while let Some(key) = map.next_key::<i128>()? {
                    match key {
                        1 => format_version = Some(map.next_value()?),
                        2 => doc_id = Some(map.next_value()?),
                        3 => content_hash = Some(map.next_value()?),
                        other => {
                            return Err(de::Error::custom(format!(
                                "unexpected protected-header key {other}"
                            )));
                        }
                    }
                }
                Ok(ProtectedHeader {
                    format_version: format_version.ok_or_else(|| de::Error::missing_field("1"))?,
                    doc_id: doc_id.ok_or_else(|| de::Error::missing_field("2"))?,
                    content_hash: content_hash
                        .ok_or_else(|| de::Error::missing_field("3"))?
                        .into_vec(),
                })
            }
        }

        deserializer.deserialize_map(ProtectedHeaderVisitor)
    }
}
