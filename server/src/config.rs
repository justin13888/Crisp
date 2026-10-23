//! Process configuration, loaded once from the environment at startup.
//!
//! Every variable read here is documented in `server/.env.example`. A
//! variable that is set but malformed is a startup error — never silently
//! replaced by its default — so a typo cannot quietly change behavior.

use std::str::FromStr;

use eyre::{Context, eyre};

/// The default maximum accepted `.crisp` upload size (16 MiB), matching the
/// spec §12 bundle ceiling.
pub const DEFAULT_MAX_BUNDLE_BYTES: u64 = 16 * 1024 * 1024;
/// The default session lifetime: 30 days.
pub const DEFAULT_SESSION_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// The default login-challenge lifetime: 5 minutes.
pub const DEFAULT_CHALLENGE_TTL_SECS: i64 = 5 * 60;
/// The default interval between expired-session/challenge purges: 10 minutes.
pub const DEFAULT_PURGE_INTERVAL_SECS: u64 = 10 * 60;
/// The default bind address.
pub const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";

/// The HTTP-surface settings the router is built from.
///
/// Kept apart from [`Config`] so the router (and the OpenAPI document it
/// describes) can be built without a database URL or signing seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpOptions {
    /// The largest `.crisp` upload accepted, in bytes.
    pub max_bundle_bytes: u64,
    /// The CORS origin allowlist. `None` means any origin is allowed (the
    /// lenient default); `Some` restricts cross-origin access to the listed
    /// origins.
    pub allowed_origins: Option<Vec<String>>,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            max_bundle_bytes: DEFAULT_MAX_BUNDLE_BYTES,
            allowed_origins: None,
        }
    }
}

/// All runtime configuration for the server.
#[derive(Debug, Clone)]
pub struct Config {
    /// The PostgreSQL connection URL (`CRISP_DATABASE_URL`).
    pub database_url: String,
    /// The `host:port` the HTTP server binds to (`CRISP_BIND_ADDR`).
    pub bind_addr: String,
    /// The 32-byte seed for the platform endorsement key
    /// (`CRISP_PLATFORM_SEED`, 64 hex chars).
    pub platform_seed: [u8; 32],
    /// How long an issued session token stays valid, in seconds
    /// (`CRISP_SESSION_TTL_SECS`).
    pub session_ttl_secs: i64,
    /// How long a login challenge stays valid, in seconds
    /// (`CRISP_CHALLENGE_TTL_SECS`).
    pub challenge_ttl_secs: i64,
    /// How often expired sessions and challenges are purged, in seconds
    /// (`CRISP_PURGE_INTERVAL_SECS`).
    pub purge_interval_secs: u64,
    /// The HTTP-surface settings (`CRISP_MAX_BUNDLE_BYTES`,
    /// `CRISP_ALLOWED_ORIGINS`).
    pub http: HttpOptions,
}

impl Config {
    /// Loads configuration from the process environment.
    ///
    /// # Errors
    ///
    /// Returns an error if a required variable is missing or any variable is
    /// malformed.
    pub fn from_env() -> eyre::Result<Self> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Loads configuration through `lookup`, which maps a variable name to
    /// its value (if set). `CRISP_DATABASE_URL` and `CRISP_PLATFORM_SEED` are
    /// required; every other setting has a documented default.
    ///
    /// # Errors
    ///
    /// Returns an error if a required variable is missing or any variable is
    /// malformed.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> eyre::Result<Self> {
        let set = |key: &str| lookup(key).filter(|value| !value.trim().is_empty());

        let database_url = set("CRISP_DATABASE_URL")
            .ok_or_else(|| eyre!("required environment variable CRISP_DATABASE_URL is not set"))?;
        let bind_addr = set("CRISP_BIND_ADDR").unwrap_or_else(|| DEFAULT_BIND_ADDR.to_owned());

        let seed_hex = set("CRISP_PLATFORM_SEED")
            .ok_or_else(|| eyre!("required environment variable CRISP_PLATFORM_SEED is not set"))?;
        let platform_seed = parse_seed(&seed_hex)?;

        let session_ttl_secs = parse_positive(
            "CRISP_SESSION_TTL_SECS",
            set("CRISP_SESSION_TTL_SECS"),
            DEFAULT_SESSION_TTL_SECS,
        )?;
        let challenge_ttl_secs = parse_positive(
            "CRISP_CHALLENGE_TTL_SECS",
            set("CRISP_CHALLENGE_TTL_SECS"),
            DEFAULT_CHALLENGE_TTL_SECS,
        )?;
        let purge_interval_secs = parse_positive(
            "CRISP_PURGE_INTERVAL_SECS",
            set("CRISP_PURGE_INTERVAL_SECS"),
            DEFAULT_PURGE_INTERVAL_SECS,
        )?;
        let max_bundle_bytes = parse_positive(
            "CRISP_MAX_BUNDLE_BYTES",
            set("CRISP_MAX_BUNDLE_BYTES"),
            DEFAULT_MAX_BUNDLE_BYTES,
        )?;
        let allowed_origins =
            parse_allowed_origins(&set("CRISP_ALLOWED_ORIGINS").unwrap_or_default())?;

        Ok(Self {
            database_url,
            bind_addr,
            platform_seed,
            session_ttl_secs,
            challenge_ttl_secs,
            purge_interval_secs,
            http: HttpOptions {
                max_bundle_bytes,
                allowed_origins,
            },
        })
    }
}

/// Decodes the 64-hex-char platform seed.
fn parse_seed(value: &str) -> eyre::Result<[u8; 32]> {
    let bytes = hex::decode(value.trim()).wrap_err("CRISP_PLATFORM_SEED is not valid hex")?;
    bytes
        .try_into()
        .map_err(|_| eyre!("CRISP_PLATFORM_SEED must decode to exactly 32 bytes"))
}

/// Parses an optional numeric variable that must be strictly positive,
/// falling back to `default` only when it is unset.
fn parse_positive<T>(key: &str, value: Option<String>, default: T) -> eyre::Result<T>
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
fn parse_allowed_origins(value: &str) -> eyre::Result<Option<Vec<String>>> {
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const SEED: &str = "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";

    fn load(vars: &[(&str, &str)]) -> eyre::Result<Config> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Config::from_lookup(|key| map.get(key).cloned())
    }

    fn base() -> Vec<(&'static str, &'static str)> {
        vec![
            ("CRISP_DATABASE_URL", "postgresql://localhost/crisp"),
            ("CRISP_PLATFORM_SEED", SEED),
        ]
    }

    #[test]
    fn defaults_apply_when_optional_variables_are_unset() {
        let config = load(&base()).unwrap();
        assert_eq!(config.bind_addr, DEFAULT_BIND_ADDR);
        assert_eq!(config.session_ttl_secs, DEFAULT_SESSION_TTL_SECS);
        assert_eq!(config.challenge_ttl_secs, DEFAULT_CHALLENGE_TTL_SECS);
        assert_eq!(config.purge_interval_secs, DEFAULT_PURGE_INTERVAL_SECS);
        assert_eq!(config.http, HttpOptions::default());
        assert_eq!(config.platform_seed, [0x2a; 32]);
    }

    #[test]
    fn explicit_values_override_defaults() {
        let mut vars = base();
        vars.extend([
            ("CRISP_SESSION_TTL_SECS", "60"),
            ("CRISP_MAX_BUNDLE_BYTES", "1024"),
            ("CRISP_PURGE_INTERVAL_SECS", "5"),
            (
                "CRISP_ALLOWED_ORIGINS",
                "http://localhost:3000, https://crisp.example",
            ),
        ]);
        let config = load(&vars).unwrap();
        assert_eq!(config.session_ttl_secs, 60);
        assert_eq!(config.http.max_bundle_bytes, 1024);
        assert_eq!(config.purge_interval_secs, 5);
        assert_eq!(
            config.http.allowed_origins,
            Some(vec![
                "http://localhost:3000".to_owned(),
                "https://crisp.example".to_owned()
            ])
        );
    }

    #[test]
    fn invalid_numbers_are_rejected_not_defaulted() {
        for key in [
            "CRISP_SESSION_TTL_SECS",
            "CRISP_CHALLENGE_TTL_SECS",
            "CRISP_MAX_BUNDLE_BYTES",
            "CRISP_PURGE_INTERVAL_SECS",
        ] {
            for bad in ["abc", "-5", "0", "1.5"] {
                let mut vars = base();
                vars.push((key, bad));
                let error = load(&vars).expect_err(&format!("{key}={bad} must be rejected"));
                assert!(error.to_string().contains(key), "{error}");
            }
        }
    }

    #[test]
    fn missing_required_variables_are_reported() {
        assert!(load(&[("CRISP_PLATFORM_SEED", SEED)]).is_err());
        assert!(load(&[("CRISP_DATABASE_URL", "postgresql://x")]).is_err());
    }

    #[test]
    fn malformed_seed_is_rejected() {
        let mut vars = base();
        vars[1] = ("CRISP_PLATFORM_SEED", "zz");
        assert!(load(&vars).is_err());
        vars[1] = ("CRISP_PLATFORM_SEED", "2a2a");
        assert!(load(&vars).is_err());
    }

    #[test]
    fn malformed_origins_are_rejected() {
        for bad in [
            "localhost:3000",
            "http://",
            "https://a.example/path",
            "ftp://x",
        ] {
            let mut vars = base();
            vars.push(("CRISP_ALLOWED_ORIGINS", bad));
            assert!(load(&vars).is_err(), "{bad} must be rejected");
        }
    }
}
