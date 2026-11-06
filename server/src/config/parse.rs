//! Parsers for the environment's value grammars, shared by every
//! configuration section: positive numbers, `REQUESTS/SECONDS` budgets,
//! and the CORS origin allowlist.

use std::num::NonZeroU32;
use std::str::FromStr;
use std::time::Duration;

use eyre::{Context, eyre};

use crate::limits::Budget;

/// Parses an optional `REQUESTS/SECONDS` budget, e.g. `10/3600`, falling
/// back to `default` only when it is unset. Both numbers must be positive.
pub(super) fn parse_budget(
    key: &str,
    value: Option<String>,
    default: Budget,
) -> eyre::Result<Budget> {
    let Some(value) = value else {
        return Ok(default);
    };
    let (requests, seconds) = value
        .trim()
        .split_once('/')
        .ok_or_else(|| eyre!("{key} must be REQUESTS/SECONDS, got {value:?}"))?;
    let requests: u32 = parse_positive(key, Some(requests.to_owned()), 1)?;
    let seconds: u64 = parse_positive(key, Some(seconds.to_owned()), 1)?;
    let requests = NonZeroU32::new(requests).ok_or_else(|| eyre!("{key} must be positive"))?;
    Ok(Budget::new(requests, Duration::from_secs(seconds)))
}

/// Parses an optional numeric variable that must be strictly positive,
/// falling back to `default` only when it is unset.
pub(super) fn parse_positive<T>(key: &str, value: Option<String>, default: T) -> eyre::Result<T>
where
    T: FromStr + PartialOrd + Default + Copy,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let Some(value) = value else {
        return Ok(default);
    };
    let parsed: T = value
        .trim()
        .parse()
        .wrap_err_with(|| format!("{key} is not a valid number: {value:?}"))?;
    if parsed <= T::default() {
        return Err(eyre!("{key} must be greater than zero, got {value:?}"));
    }
    Ok(parsed)
}

/// Parses the `CRISP_ALLOWED_ORIGINS` value into a CORS origin allowlist.
///
/// An empty value yields `None` (any origin allowed). Otherwise the value is a
/// comma-separated list of origins, each of which must be an
/// `http(s)://host[:port]` origin with no path.
///
/// # Errors
///
/// Returns an error if a listed origin is malformed.
pub(super) fn parse_allowed_origins(value: &str) -> eyre::Result<Option<Vec<String>>> {
    let origins: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(|origin| {
            let rest = origin
                .strip_prefix("https://")
                .or_else(|| origin.strip_prefix("http://"));
            match rest {
                Some(host) if !host.is_empty() && !host.contains('/') && origin.is_ascii() => {
                    Ok(origin.to_owned())
                }
                _ => Err(eyre!(
                    "CRISP_ALLOWED_ORIGINS contains an invalid origin {origin:?} \
                     (expected scheme://host[:port])"
                )),
            }
        })
        .collect::<eyre::Result<_>>()?;
    Ok((!origins.is_empty()).then_some(origins))
}
