//! Reactive moderation (`docs/public-server-policy.md` §4): accounts report
//! documents, and administrators resolve reports, take documents down, and
//! suspend accounts.
//!
//! - [`reports`] — `POST /reports`, and the administrators' report queue.
//! - [`takedowns`] — removing a document or revision whoever published it,
//!   leaving a tombstone, and lifting it again.
//! - [`accounts`] — suspending and unsuspending an account.
//!
//! An administrator is an account `CRISP_ADMIN_USR_IDS` lists; every `/admin`
//! route takes an [`AdminSession`](crate::auth::AdminSession) guard, which
//! refuses any other session 403. Each administrator action writes an
//! append-only `moderation_actions` row in the transaction that performs it,
//! and [`log_action`] logs that row at INFO.

use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::db::moderation_action;
use crate::error::Internal;

pub mod accounts;
pub mod reports;
pub mod takedowns;

/// Why something is reported, taken down, or suspended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    /// The content infringes someone's copyright.
    Copyright,
    /// The content is illegal.
    Illegal,
    /// The content is spam.
    Spam,
    /// The content harasses someone.
    Harassment,
    /// Anything else; the detail explains.
    Other,
}

impl Category {
    /// Every category, in declaration order.
    pub const ALL: [Self; 5] = [
        Self::Copyright,
        Self::Illegal,
        Self::Spam,
        Self::Harassment,
        Self::Other,
    ];

    /// The category as stored and sent.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Copyright => "copyright",
            Self::Illegal => "illegal",
            Self::Spam => "spam",
            Self::Harassment => "harassment",
            Self::Other => "other",
        }
    }

    /// Reads a stored category back.
    ///
    /// # Errors
    ///
    /// Returns [`Internal`] for a value no version of the server writes.
    pub fn stored(value: &str) -> Result<Self, Internal> {
        Self::ALL
            .into_iter()
            .find(|category| category.as_str() == value)
            .ok_or_else(|| Internal::logged("an unknown stored category", &value))
    }
}

/// Logs one audit row at INFO, with the fields it stores.
pub fn log_action(entry: &moderation_action::Model) {
    tracing::info!(
        audit_id = entry.id,
        actor = %entry.actor,
        action = %entry.action,
        target = %entry.target,
        reason = entry.reason.as_deref(),
        report_id = entry.report_id,
        at = entry.created_at,
        "moderation action"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_round_trip_through_their_stored_form() {
        for category in Category::ALL {
            assert_eq!(Category::stored(category.as_str()).unwrap(), category);
            let json = serde_json::to_value(category).unwrap();
            assert_eq!(json, category.as_str());
        }
        assert!(Category::stored("rude").is_err());
    }
}
