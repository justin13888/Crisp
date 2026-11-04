//! The retired platform keys (`CRISP_PLATFORM_RETIRED_SEEDS`).

use eyre::{Context, eyre};

/// The variable naming the retired platform keys the server still holds.
const RETIRED_VAR: &str = "CRISP_PLATFORM_RETIRED_SEEDS";

/// A retired platform key the server still holds: published with its
/// `valid_until`, trusted for endorsements dated inside its window, and used
/// to sign the key set so clients that pinned it accept the new current key
/// (spec §8.5).
#[derive(Clone, PartialEq, Eq)]
pub struct RetiredKey {
    /// The key's 32-byte secret seed.
    pub seed: [u8; 32],
    /// The last instant, in Unix seconds, the key may have signed.
    pub valid_until: u64,
}

impl std::fmt::Debug for RetiredKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the seed.
        f.debug_struct("RetiredKey")
            .field("valid_until", &self.valid_until)
            .finish_non_exhaustive()
    }
}

/// Decodes a 64-hex-char seed named by `key`.
pub(super) fn parse_seed(key: &str, value: &str) -> eyre::Result<[u8; 32]> {
    let bytes = hex::decode(value.trim()).wrap_err_with(|| format!("{key} is not valid hex"))?;
    bytes
        .try_into()
        .map_err(|_| eyre!("{key} must decode to exactly 32 bytes"))
}

/// Reads `CRISP_PLATFORM_RETIRED_SEEDS`: a comma-separated list of
/// `SEED_HEX@VALID_UNTIL` entries, `VALID_UNTIL` in Unix seconds. Unset means
/// no retired key. No entry may repeat a seed or reuse `current`.
///
/// # Errors
///
/// Returns an error if an entry is malformed or a seed repeats.
pub(super) fn retired_keys(
    value: Option<String>,
    current: &[u8; 32],
) -> eyre::Result<Vec<RetiredKey>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let mut keys: Vec<RetiredKey> = Vec::new();
    for entry in value.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let (seed, until) = entry
            .split_once('@')
            .ok_or_else(|| eyre!("{RETIRED_VAR} entries must be SEED_HEX@VALID_UNTIL"))?;
        let seed = parse_seed(RETIRED_VAR, seed)?;
        let valid_until: u64 = until
            .trim()
            .parse()
            .ok()
            .filter(|t| *t > 0)
            .ok_or_else(|| eyre!("{RETIRED_VAR} VALID_UNTIL must be positive Unix seconds"))?;
        if &seed == current || keys.iter().any(|k| k.seed == seed) {
            return Err(eyre!(
                "{RETIRED_VAR} repeats a seed (or the current CRISP_PLATFORM_SEED)"
            ));
        }
        keys.push(RetiredKey { seed, valid_until });
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT: [u8; 32] = [0x2a; 32];

    fn hex_seed(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    #[test]
    fn unset_means_no_retired_key() {
        assert!(retired_keys(None, &CURRENT).unwrap().is_empty());
        assert!(
            retired_keys(Some(" , ".into()), &CURRENT)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn entries_are_read_in_order() {
        let value = format!("{}@100, {}@200", hex_seed(1), hex_seed(2));
        let keys = retired_keys(Some(value), &CURRENT).unwrap();
        assert_eq!(
            keys,
            [
                RetiredKey {
                    seed: [1; 32],
                    valid_until: 100
                },
                RetiredKey {
                    seed: [2; 32],
                    valid_until: 200
                },
            ]
        );
    }

    #[test]
    fn malformed_entries_are_rejected() {
        let one = hex_seed(1);
        for bad in [
            one.clone(),
            format!("{one}@"),
            format!("{one}@0"),
            format!("{one}@-5"),
            format!("{one}@soon"),
            "zz@100".to_owned(),
            "2a2a@100".to_owned(),
            format!("{one}@100,{one}@200"),
            format!("{}@100", hex_seed(0x2a)),
        ] {
            let error = retired_keys(Some(bad.clone()), &CURRENT).expect_err(&bad);
            assert!(error.to_string().contains(RETIRED_VAR), "{error}");
        }
    }

    #[test]
    fn debug_output_omits_the_seed() {
        let key = RetiredKey {
            seed: [0xab; 32],
            valid_until: 7,
        };
        let shown = format!("{key:?}");
        assert!(!shown.contains("171") && !shown.contains("ab"), "{shown}");
    }
}
