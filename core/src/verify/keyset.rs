//! Platform key-set statements and the pin-update rule (spec §8.5).
//!
//! A server publishes its endorsement key set together with signatures over
//! that set by keys it holds. A client that pinned an earlier set accepts the
//! new one only when a key it pinned as current vouches for it, so a key can
//! be added or retired without the cook re-trusting the server by hand, while
//! anyone who merely answers for the server's address cannot slip a key in.

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::cbor::{Value, encode_canonical};
use crate::codec::to_canonical_cbor;
use crate::error::Result;
use crate::identity::key_fingerprint;
use crate::sign::{SecretKeySet, verify_composite};
use crate::verify::PlatformKey;

/// The domain separator for a platform key-set signature.
const KEY_SET_DOMAIN: &str = "crisp/platform-key-set/1";

/// Builds the message a key-set signature covers:
/// `canonical_cbor(["crisp/platform-key-set/1", .cbor platform-key-set])`,
/// where the set is the keys in the order the server publishes them.
///
/// # Errors
///
/// Returns an error if canonical CBOR encoding fails.
pub fn key_set_message(keys: &[PlatformKey]) -> Result<Vec<u8>> {
    encode_canonical(&Value::Array(vec![
        Value::Text(KEY_SET_DOMAIN.to_owned()),
        Value::Bytes(to_canonical_cbor(&keys)?),
    ]))
}

/// Signs the key set `keys` with `secret` (a key the server holds).
///
/// # Errors
///
/// Returns an error if encoding or signing fails.
pub fn sign_key_set(secret: &SecretKeySet, keys: &[PlatformKey]) -> Result<Vec<u8>> {
    secret.sign(&key_set_message(keys)?)
}

/// What a client does with a server's key set, given the set it pinned.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PinUpdate {
    /// The offered set is the pinned set: keep the pin.
    Unchanged,
    /// Pin `keys` in place of the old pin (first contact, or a vouched change).
    Updated {
        /// The new pinned set.
        keys: Vec<PlatformKey>,
    },
    /// The offered set breaks the rule: keep the old pin and warn the cook.
    Rejected {
        /// Why the change was refused.
        reason: String,
    },
}

/// The fingerprint a key is matched by, or `None` for a malformed key.
fn fingerprint(key: &PlatformKey) -> Option<[u8; 32]> {
    key_fingerprint(&key.keyset).ok()
}

/// The key in `keys` with the same fingerprint as `key`.
fn find<'a>(keys: &'a [PlatformKey], key: &PlatformKey) -> Option<&'a PlatformKey> {
    let id = fingerprint(key)?;
    keys.iter().find(|k| fingerprint(k) == Some(id))
}

/// Whether `offered`'s window lies inside `pinned`'s (a pin may only narrow).
fn narrows(pinned: &PlatformKey, offered: &PlatformKey) -> bool {
    let until_ok = match (pinned.valid_until, offered.valid_until) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(old), Some(new)) => new <= old,
    };
    offered.valid_from >= pinned.valid_from && until_ok
}

/// Whether `a` and `b` hold the same keys with the same windows.
fn same_set(a: &[PlatformKey], b: &[PlatformKey]) -> bool {
    a.len() == b.len() && a.iter().all(|k| find(b, k) == Some(k))
}

/// Applies the spec §8.5 pin-update rule to a server's `offered` key set and
/// its key-set `signatures`, against the set `pinned` for that server.
///
/// - Nothing pinned yet: pin `offered` (first contact).
/// - `offered` equals `pinned`: unchanged.
/// - Otherwise the change is accepted only when every key pinned as current
///   (no `valid_until`) is still offered, no pinned key's window widens, and
///   a signature over `offered` verifies under a key pinned as current. The
///   new pin is `offered` plus every pinned key the server no longer
///   publishes, which keeps verifying the endorsements it made.
#[must_use]
pub fn update_pin(
    pinned: &[PlatformKey],
    offered: &[PlatformKey],
    signatures: &[Vec<u8>],
) -> PinUpdate {
    let rejected = |reason: &str| {
        info!(reason, "platform key set change rejected");
        PinUpdate::Rejected {
            reason: reason.to_owned(),
        }
    };
    if offered.is_empty() {
        return rejected("the server publishes no endorsement key");
    }
    if pinned.is_empty() {
        debug!(
            keys = offered.len(),
            "pinning a server's key set on first contact"
        );
        return PinUpdate::Updated {
            keys: offered.to_vec(),
        };
    }
    if same_set(pinned, offered) {
        return PinUpdate::Unchanged;
    }
    for old in pinned {
        match find(offered, old) {
            None if old.valid_until.is_none() => {
                return rejected("a current pinned key is missing from the server's set");
            }
            Some(new) if !narrows(old, new) => {
                return rejected("the server's set widens a pinned key's validity window");
            }
            _ => {}
        }
    }
    let Ok(message) = key_set_message(offered) else {
        return rejected("the server's key set cannot be encoded");
    };
    let vouched = pinned.iter().filter(|k| k.valid_until.is_none()).any(|k| {
        signatures
            .iter()
            .any(|sig| verify_composite(&k.keyset, &message, sig).is_ok())
    });
    if !vouched {
        return rejected("no current pinned key signed the server's new key set");
    }
    let mut keys = offered.to_vec();
    keys.extend(
        pinned
            .iter()
            .filter(|k| find(offered, k).is_none())
            .cloned(),
    );
    info!(
        keys = keys.len(),
        "accepted a vouched platform key set change"
    );
    PinUpdate::Updated { keys }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::primitives::Timestamp;

    fn secret(byte: u8) -> SecretKeySet {
        SecretKeySet::from_seed(&[byte; 32])
    }

    fn key(byte: u8, valid_until: Option<u64>) -> PlatformKey {
        PlatformKey {
            keyset: secret(byte).public_keyset(),
            valid_from: Timestamp(0),
            valid_until: valid_until.map(Timestamp),
        }
    }

    fn signed(by: &[u8], keys: &[PlatformKey]) -> Vec<Vec<u8>> {
        by.iter()
            .map(|b| sign_key_set(&secret(*b), keys).unwrap())
            .collect()
    }

    /// A rotation from key 1 to key 2: key 2 current, key 1 retired at 100.
    fn rotated() -> Vec<PlatformKey> {
        vec![key(2, None), key(1, Some(100))]
    }

    #[test]
    fn first_contact_pins_the_offered_set() {
        let offered = vec![key(1, None)];
        assert_eq!(
            update_pin(&[], &offered, &[]),
            PinUpdate::Updated { keys: offered }
        );
    }

    #[test]
    fn an_empty_offered_set_is_rejected() {
        assert!(matches!(
            update_pin(&[key(1, None)], &[], &[]),
            PinUpdate::Rejected { .. }
        ));
        assert!(matches!(
            update_pin(&[], &[], &[]),
            PinUpdate::Rejected { .. }
        ));
    }

    #[test]
    fn the_same_set_in_any_order_is_unchanged() {
        let pinned = rotated();
        let reordered = vec![key(1, Some(100)), key(2, None)];
        assert_eq!(update_pin(&pinned, &reordered, &[]), PinUpdate::Unchanged);
    }

    #[test]
    fn a_rotation_vouched_by_the_pinned_key_is_accepted() {
        let offered = rotated();
        let outcome = update_pin(&[key(1, None)], &offered, &signed(&[1], &offered));
        assert_eq!(outcome, PinUpdate::Updated { keys: offered });
    }

    #[test]
    fn a_signature_by_the_new_key_alone_does_not_vouch() {
        let offered = rotated();
        let outcome = update_pin(&[key(1, None)], &offered, &signed(&[2], &offered));
        assert!(matches!(outcome, PinUpdate::Rejected { .. }), "{outcome:?}");
    }

    #[test]
    fn an_unsigned_addition_is_rejected() {
        let offered = vec![key(1, None), key(9, None)];
        let outcome = update_pin(&[key(1, None)], &offered, &[]);
        assert!(matches!(outcome, PinUpdate::Rejected { .. }), "{outcome:?}");
    }

    #[test]
    fn a_signature_over_another_set_does_not_vouch() {
        let offered = rotated();
        let other = vec![key(2, None), key(1, Some(200))];
        let outcome = update_pin(&[key(1, None)], &offered, &signed(&[1], &other));
        assert!(matches!(outcome, PinUpdate::Rejected { .. }), "{outcome:?}");
    }

    #[test]
    fn a_retired_pinned_key_cannot_vouch() {
        let pinned = rotated();
        let offered = vec![key(3, None), key(2, None), key(1, Some(100))];
        let by_retired = update_pin(&pinned, &offered, &signed(&[1], &offered));
        assert!(matches!(by_retired, PinUpdate::Rejected { .. }));
        let by_current = update_pin(&pinned, &offered, &signed(&[2], &offered));
        assert_eq!(by_current, PinUpdate::Updated { keys: offered });
    }

    #[test]
    fn dropping_a_current_pinned_key_is_rejected_even_when_vouched() {
        let offered = vec![key(2, None)];
        let outcome = update_pin(&[key(1, None)], &offered, &signed(&[1, 2], &offered));
        assert!(matches!(outcome, PinUpdate::Rejected { .. }), "{outcome:?}");
    }

    #[test]
    fn a_dropped_retired_key_stays_pinned() {
        let pinned = rotated();
        let offered = vec![key(2, None)];
        let outcome = update_pin(&pinned, &offered, &signed(&[2], &offered));
        assert_eq!(
            outcome,
            PinUpdate::Updated {
                keys: vec![key(2, None), key(1, Some(100))]
            }
        );
    }

    #[test]
    fn widening_a_pinned_window_is_rejected_even_when_vouched() {
        let pinned = rotated();
        for widened in [None, Some(200)] {
            let offered = vec![key(2, None), key(1, widened)];
            let outcome = update_pin(&pinned, &offered, &signed(&[2], &offered));
            assert!(matches!(outcome, PinUpdate::Rejected { .. }), "{widened:?}");
        }
        let mut later_start = key(2, None);
        later_start.valid_from = Timestamp(5);
        let mut earlier_start = rotated();
        earlier_start[0].valid_from = Timestamp(10);
        let offered = vec![later_start, key(1, Some(100))];
        let outcome = update_pin(&earlier_start, &offered, &signed(&[2], &offered));
        assert!(matches!(outcome, PinUpdate::Rejected { .. }), "{outcome:?}");
    }

    #[test]
    fn narrowing_a_retired_window_is_accepted_when_vouched() {
        let pinned = rotated();
        let offered = vec![key(2, None), key(1, Some(50))];
        let outcome = update_pin(&pinned, &offered, &signed(&[2], &offered));
        assert_eq!(outcome, PinUpdate::Updated { keys: offered });
    }

    #[test]
    fn the_key_set_message_is_domain_separated_and_order_sensitive() {
        let a = key_set_message(&rotated()).unwrap();
        let b = key_set_message(&[key(1, Some(100)), key(2, None)]).unwrap();
        assert_ne!(a, b);
        let signing = crate::sign::signing_message(b"", b"", b"").unwrap();
        assert_ne!(a, signing);
        assert_eq!(a, key_set_message(&rotated()).unwrap());
    }
}
