//! The account-profile data model (`crisp-v1.cddl`: `account-profile`).

use serde::{Deserialize, Serialize};

use crate::model::meta::PublicKeySet;
use crate::model::primitives::Timestamp;

/// A self-asserted account profile: a username and display name bound to a
/// composite public key (`crisp-v1.cddl`: `account-profile`).
///
/// A recipe's author entry is already covered by the bundle signature, but a
/// *server account's* display name would otherwise be an unsigned string the
/// server simply trusts. Wrapping this profile in a [`crate::profile::SignedProfile`]
/// makes the name self-asserted — signed by the very key it describes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountProfile {
    /// The composite public key this profile describes and is signed by.
    pub keyset: PublicKeySet,
    /// The chosen server account username.
    pub username: String,
    /// A human-readable display name.
    pub display_name: String,
    /// When this profile was signed (epoch seconds).
    pub signed_at: Timestamp,
}
