//! Stable error codes and the crate-wide error type.
//!
//! Every way a Crisp bundle can fail verification has a **stable string code**
//! (spec §11, "E.\*"). These codes are part of the format's public contract:
//! the compliance suite (`invalid/<CODE>/*.crisp`) is keyed by them, so they
//! must never change spelling. [`ErrorCode`] is the exhaustive, closed set.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A convenient `Result` alias defaulting to [`CrispError`].
pub type Result<T, E = CrispError> = core::result::Result<T, E>;

/// The verification phase a check belongs to (spec §11).
///
/// Phases `A` and `B` are *fatal* — verification stops on the first failure.
/// Phases `C`, `D`, and `E` are *collect-all* — every failure is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Phase A — framing & encoding.
    A,
    /// Phase B — schema & limits.
    B,
    /// Phase C — integrity & identity.
    C,
    /// Phase D — policy.
    D,
    /// Phase E — signatures (cryptographic).
    E,
}

impl Phase {
    /// Whether a failure in this phase aborts verification immediately.
    #[must_use]
    pub const fn is_fatal(self) -> bool {
        matches!(self, Phase::A | Phase::B)
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = match self {
            Phase::A => 'A',
            Phase::B => 'B',
            Phase::C => 'C',
            Phase::D => 'D',
            Phase::E => 'E',
        };
        write!(f, "{c}")
    }
}

/// A stable verification error code, exactly as defined by spec §11.
///
/// The string form (e.g. `"E.ID.MISMATCH"`) is the contract; obtain it with
/// [`ErrorCode::as_str`] and parse it back with [`FromStr`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// Self-describe or application tag missing/incorrect (A1).
    Magic,
    /// Unsupported container version (A2).
    Container,
    /// The file is not well-formed CBOR (A3).
    CborMalformed,
    /// CBOR is well-formed but not in canonical/deterministic form (A4/A5).
    CborNonDeterministic,
    /// An indefinite-length CBOR item was present (A4).
    CborIndefinite,
    /// A CBOR map contained duplicate keys (A4).
    CborDupKey,
    /// A CBOR tag outside the allowlist `{55799, 1129468752}` was present (A4).
    CborTag,
    /// The envelope or payload violates `crisp-v1.cddl` (B1).
    Schema,
    /// The format version is outside the reader's supported range (B2).
    Version,
    /// A spec §12 hard limit was exceeded (B3).
    Limit,
    /// `kind`/`book`/`meta.identity`/`default_language` are inconsistent (B4).
    Consistency,
    /// A blob's id does not equal `SHA-256(bytes)` (C1).
    BlobHash,
    /// A blob's declared `size` does not equal its byte length (C1).
    BlobSize,
    /// A blob's `media_type` is not allowlisted (C1).
    BlobMediaType,
    /// An id or revision has an invalid bech32m checksum or wrong HRP (C2).
    IdChecksum,
    /// A recipe's `id` does not equal `derive-recipe-id(identity)` (C3).
    IdMismatch,
    /// A recipe's `revision` does not equal `derive-revision(recipe)` (C3).
    RevMismatch,
    /// The protected-header content hash does not equal `SHA-256(payload)` (C4).
    ContentHash,
    /// A recipe's `author_fpr` matches no `meta.authors[].keyset` (C5).
    AuthorBinding,
    /// An internal reference (`ingredient_refs`, indices, …) does not resolve (C6).
    Ref,
    /// A `run.link` does not use the `https` scheme or is malformed (D1).
    UrlScheme,
    /// A localized map is missing the bundle's `default_language` key (D2).
    L10nDefault,
    /// A localized map uses a language not declared in `languages` (D2).
    L10nUndeclared,
    /// A `created_at`/`modified_at`/endorsement time ordering was violated (D3/E3).
    TimeOrder,
    /// A `quantity` has neither `min` nor `text` (D4).
    Quantity,
    /// A required signature (author or server endorsement) is missing (E1/E2).
    SigMissing,
    /// An author signature failed Composite ML-DSA verification (E1).
    SigAuthor,
    /// An endorsement `kid` does not resolve to a trusted platform key (E2).
    KeyUnknown,
    /// An endorsement key is outside its validity window (E2).
    KeyWindow,
    /// The server endorsement failed Composite ML-DSA verification (E2).
    SigEndorsement,
    /// The endorsement's `external_aad` does not bind the author signature (E2).
    SigBinding,
}

impl ErrorCode {
    /// Every error code, in spec order. Used by the compliance suite and tests.
    pub const ALL: [ErrorCode; 31] = [
        ErrorCode::Magic,
        ErrorCode::Container,
        ErrorCode::CborMalformed,
        ErrorCode::CborNonDeterministic,
        ErrorCode::CborIndefinite,
        ErrorCode::CborDupKey,
        ErrorCode::CborTag,
        ErrorCode::Schema,
        ErrorCode::Version,
        ErrorCode::Limit,
        ErrorCode::Consistency,
        ErrorCode::BlobHash,
        ErrorCode::BlobSize,
        ErrorCode::BlobMediaType,
        ErrorCode::IdChecksum,
        ErrorCode::IdMismatch,
        ErrorCode::RevMismatch,
        ErrorCode::ContentHash,
        ErrorCode::AuthorBinding,
        ErrorCode::Ref,
        ErrorCode::UrlScheme,
        ErrorCode::L10nDefault,
        ErrorCode::L10nUndeclared,
        ErrorCode::TimeOrder,
        ErrorCode::Quantity,
        ErrorCode::SigMissing,
        ErrorCode::SigAuthor,
        ErrorCode::KeyUnknown,
        ErrorCode::KeyWindow,
        ErrorCode::SigEndorsement,
        ErrorCode::SigBinding,
    ];

    /// The stable string form of this code, e.g. `"E.ID.MISMATCH"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::Magic => "E.MAGIC",
            ErrorCode::Container => "E.CONTAINER",
            ErrorCode::CborMalformed => "E.CBOR.MALFORMED",
            ErrorCode::CborNonDeterministic => "E.CBOR.NONDETERMINISTIC",
            ErrorCode::CborIndefinite => "E.CBOR.INDEFINITE",
            ErrorCode::CborDupKey => "E.CBOR.DUPKEY",
            ErrorCode::CborTag => "E.CBOR.TAG",
            ErrorCode::Schema => "E.SCHEMA",
            ErrorCode::Version => "E.VERSION",
            ErrorCode::Limit => "E.LIMIT",
            ErrorCode::Consistency => "E.CONSISTENCY",
            ErrorCode::BlobHash => "E.BLOB.HASH",
            ErrorCode::BlobSize => "E.BLOB.SIZE",
            ErrorCode::BlobMediaType => "E.BLOB.MEDIATYPE",
            ErrorCode::IdChecksum => "E.ID.CHECKSUM",
            ErrorCode::IdMismatch => "E.ID.MISMATCH",
            ErrorCode::RevMismatch => "E.REV.MISMATCH",
            ErrorCode::ContentHash => "E.CONTENT.HASH",
            ErrorCode::AuthorBinding => "E.AUTHOR.BINDING",
            ErrorCode::Ref => "E.REF",
            ErrorCode::UrlScheme => "E.URL.SCHEME",
            ErrorCode::L10nDefault => "E.L10N.DEFAULT",
            ErrorCode::L10nUndeclared => "E.L10N.UNDECLARED",
            ErrorCode::TimeOrder => "E.TIME.ORDER",
            ErrorCode::Quantity => "E.QUANTITY",
            ErrorCode::SigMissing => "E.SIG.MISSING",
            ErrorCode::SigAuthor => "E.SIG.AUTHOR",
            ErrorCode::KeyUnknown => "E.KEY.UNKNOWN",
            ErrorCode::KeyWindow => "E.KEY.WINDOW",
            ErrorCode::SigEndorsement => "E.SIG.ENDORSEMENT",
            ErrorCode::SigBinding => "E.SIG.BINDING",
        }
    }

    /// The phase this code is first raised in.
    ///
    /// A few codes (`E.TIME.ORDER`, `E.SIG.MISSING`) can arise in more than one
    /// phase; this returns the earliest. The precise check id (`D3`, `E3`, …)
    /// lives on the verification [`crate::verify`] report, not here.
    #[must_use]
    pub const fn phase(self) -> Phase {
        match self {
            ErrorCode::Magic
            | ErrorCode::Container
            | ErrorCode::CborMalformed
            | ErrorCode::CborNonDeterministic
            | ErrorCode::CborIndefinite
            | ErrorCode::CborDupKey
            | ErrorCode::CborTag => Phase::A,
            ErrorCode::Schema | ErrorCode::Version | ErrorCode::Limit | ErrorCode::Consistency => {
                Phase::B
            }
            ErrorCode::BlobHash
            | ErrorCode::BlobSize
            | ErrorCode::BlobMediaType
            | ErrorCode::IdChecksum
            | ErrorCode::IdMismatch
            | ErrorCode::RevMismatch
            | ErrorCode::ContentHash
            | ErrorCode::AuthorBinding
            | ErrorCode::Ref => Phase::C,
            ErrorCode::UrlScheme
            | ErrorCode::L10nDefault
            | ErrorCode::L10nUndeclared
            | ErrorCode::TimeOrder
            | ErrorCode::Quantity => Phase::D,
            ErrorCode::SigMissing
            | ErrorCode::SigAuthor
            | ErrorCode::KeyUnknown
            | ErrorCode::KeyWindow
            | ErrorCode::SigEndorsement
            | ErrorCode::SigBinding => Phase::E,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for ErrorCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Error returned when a string is not a recognised [`ErrorCode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownErrorCode(pub String);

impl fmt::Display for UnknownErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown Crisp error code: {}", self.0)
    }
}

impl std::error::Error for UnknownErrorCode {}

impl FromStr for ErrorCode {
    type Err = UnknownErrorCode;

    fn from_str(s: &str) -> core::result::Result<Self, Self::Err> {
        ErrorCode::ALL
            .into_iter()
            .find(|code| code.as_str() == s)
            .ok_or_else(|| UnknownErrorCode(s.to_owned()))
    }
}

/// The crate-wide error type.
///
/// A [`CrispError::Check`] carries a stable [`ErrorCode`] so callers — and the
/// verification report — can branch on the precise failure. The other variants
/// cover operational failures that are not part of the bundle-verification
/// contract (cryptography, malformed API input).
#[derive(Debug, Clone, thiserror::Error)]
pub enum CrispError {
    /// A bundle violated a verification check; carries its stable code.
    #[error("[{code}] {detail}")]
    Check {
        /// The stable verification error code.
        code: ErrorCode,
        /// Human-readable context for the failure.
        detail: String,
    },
    /// A cryptographic operation (key derivation, signing) failed.
    #[error("cryptographic operation failed: {0}")]
    Crypto(String),
    /// An argument supplied to an authoring/build API was invalid.
    #[error("invalid input: {0}")]
    Input(String),
}

impl CrispError {
    /// Builds a [`CrispError::Check`] from a code and a context message.
    #[must_use]
    pub fn check(code: ErrorCode, detail: impl Into<String>) -> Self {
        CrispError::Check {
            code,
            detail: detail.into(),
        }
    }

    /// Builds a [`CrispError::Crypto`].
    #[must_use]
    pub fn crypto(detail: impl Into<String>) -> Self {
        CrispError::Crypto(detail.into())
    }

    /// Builds a [`CrispError::Input`].
    #[must_use]
    pub fn input(detail: impl Into<String>) -> Self {
        CrispError::Input(detail.into())
    }

    /// The stable verification code, if this error is a [`CrispError::Check`].
    #[must_use]
    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            CrispError::Check { code, .. } => Some(*code),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_round_trips_through_its_string() {
        for code in ErrorCode::ALL {
            let s = code.as_str();
            assert!(s.starts_with("E."), "{s} must start with E.");
            assert_eq!(ErrorCode::from_str(s), Ok(code));
        }
    }

    #[test]
    fn all_codes_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for code in ErrorCode::ALL {
            assert!(seen.insert(code.as_str()), "duplicate code {}", code);
        }
        assert_eq!(seen.len(), 31, "spec §11 defines exactly 31 error codes");
    }

    #[test]
    fn unknown_string_is_rejected() {
        assert_eq!(
            ErrorCode::from_str("E.NOPE"),
            Err(UnknownErrorCode("E.NOPE".to_owned()))
        );
        assert!(
            ErrorCode::from_str("e.magic").is_err(),
            "codes are case-sensitive"
        );
    }

    #[test]
    fn phase_a_and_b_are_fatal_others_are_not() {
        assert!(Phase::A.is_fatal());
        assert!(Phase::B.is_fatal());
        assert!(!Phase::C.is_fatal());
        assert!(!Phase::D.is_fatal());
        assert!(!Phase::E.is_fatal());
    }

    #[test]
    fn representative_codes_map_to_expected_phases() {
        assert_eq!(ErrorCode::Magic.phase(), Phase::A);
        assert_eq!(ErrorCode::Schema.phase(), Phase::B);
        assert_eq!(ErrorCode::IdMismatch.phase(), Phase::C);
        assert_eq!(ErrorCode::Quantity.phase(), Phase::D);
        assert_eq!(ErrorCode::SigEndorsement.phase(), Phase::E);
    }

    #[test]
    fn check_error_displays_code_and_detail() {
        let err = CrispError::check(ErrorCode::IdMismatch, "recipe rcp1… id mismatch");
        assert_eq!(err.code(), Some(ErrorCode::IdMismatch));
        assert!(err.to_string().contains("E.ID.MISMATCH"));
        assert!(err.to_string().contains("id mismatch"));
    }

    #[test]
    fn operational_errors_carry_no_code() {
        assert_eq!(CrispError::crypto("boom").code(), None);
        assert_eq!(CrispError::input("bad").code(), None);
    }
}
