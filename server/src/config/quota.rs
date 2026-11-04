//! The storage-quota settings (`CRISP_QUOTA_*`, `CRISP_MAX_STORED_BYTES`).

use eyre::eyre;

use super::parse_positive;
use crate::quota::{DEFAULT_QUOTA_BUNDLES_PER_ACCOUNT, DEFAULT_QUOTA_BYTES_PER_ACCOUNT, Quota};

/// Reads the quotas through `set`. Each value must be a positive integer;
/// `CRISP_MAX_STORED_BYTES` unset means no server-wide cap. The per-account
/// byte quota must be at least `max_bundle_bytes`, so every bundle the server
/// accepts can be stored by an account with room.
///
/// # Errors
///
/// Returns an error if a value is malformed or the byte quota is smaller
/// than `max_bundle_bytes`.
pub(super) fn quota(
    set: &impl Fn(&str) -> Option<String>,
    max_bundle_bytes: u64,
) -> eyre::Result<Quota> {
    let bytes_per_account = parse_positive(
        "CRISP_QUOTA_BYTES_PER_ACCOUNT",
        set("CRISP_QUOTA_BYTES_PER_ACCOUNT"),
        DEFAULT_QUOTA_BYTES_PER_ACCOUNT,
    )?;
    if bytes_per_account < max_bundle_bytes {
        return Err(eyre!(
            "CRISP_QUOTA_BYTES_PER_ACCOUNT ({bytes_per_account}) must be at least \
             CRISP_MAX_BUNDLE_BYTES ({max_bundle_bytes})"
        ));
    }
    let bundles_per_account = parse_positive(
        "CRISP_QUOTA_BUNDLES_PER_ACCOUNT",
        set("CRISP_QUOTA_BUNDLES_PER_ACCOUNT"),
        DEFAULT_QUOTA_BUNDLES_PER_ACCOUNT,
    )?;
    let max_stored_bytes = match set("CRISP_MAX_STORED_BYTES") {
        None => None,
        value => Some(parse_positive("CRISP_MAX_STORED_BYTES", value, 1)?),
    };
    Ok(Quota {
        bytes_per_account,
        bundles_per_account,
        max_stored_bytes,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn load(vars: &[(&str, &str)], max_bundle_bytes: u64) -> eyre::Result<Quota> {
        let map: HashMap<&str, &str> = vars.iter().copied().collect();
        quota(
            &|key| map.get(key).map(|v| (*v).to_owned()),
            max_bundle_bytes,
        )
    }

    #[test]
    fn unset_values_take_the_defaults() {
        assert_eq!(load(&[], 1024).unwrap(), Quota::default());
    }

    #[test]
    fn explicit_values_are_read() {
        let quota = load(
            &[
                ("CRISP_QUOTA_BYTES_PER_ACCOUNT", "2048"),
                ("CRISP_QUOTA_BUNDLES_PER_ACCOUNT", "7"),
                ("CRISP_MAX_STORED_BYTES", "99999"),
            ],
            1024,
        )
        .unwrap();
        assert_eq!(
            quota,
            Quota {
                bytes_per_account: 2048,
                bundles_per_account: 7,
                max_stored_bytes: Some(99999),
            }
        );
    }

    #[test]
    fn malformed_values_are_startup_errors() {
        for key in [
            "CRISP_QUOTA_BYTES_PER_ACCOUNT",
            "CRISP_QUOTA_BUNDLES_PER_ACCOUNT",
            "CRISP_MAX_STORED_BYTES",
        ] {
            for bad in ["abc", "-5", "0", "1.5"] {
                let error = load(&[(key, bad)], 1).expect_err(&format!("{key}={bad}"));
                assert!(error.to_string().contains(key), "{error}");
            }
        }
    }

    #[test]
    fn the_byte_quota_must_hold_one_maximal_bundle() {
        let vars = [("CRISP_QUOTA_BYTES_PER_ACCOUNT", "1023")];
        let error = load(&vars, 1024).unwrap_err();
        assert!(
            error.to_string().contains("CRISP_MAX_BUNDLE_BYTES"),
            "{error}"
        );
        assert_eq!(load(&vars, 1023).unwrap().bytes_per_account, 1023);
    }
}
