//! The HTTP-surface settings: what the router enforces per request, and the
//! connection limits and timeouts the listener holds every socket to.

use std::net::IpAddr;
use std::num::NonZeroUsize;
use std::time::Duration;

use eyre::eyre;
use kynos::http::forwarded::{Network, ProxyHeader, TrustedProxies};

use super::DEFAULT_MAX_BUNDLE_BYTES;
use crate::limits::RateLimits;

/// The default time a client has to send a request head, and the longest an
/// idle keep-alive connection is held open: 30 seconds.
pub const DEFAULT_HEADER_TIMEOUT_SECS: u64 = 30;
/// The default time a request body may go without a byte arriving before the
/// request is answered 408: 30 seconds.
pub const DEFAULT_BODY_IDLE_TIMEOUT_SECS: u64 = 30;
/// The default cap on open connections.
pub const DEFAULT_MAX_CONNECTIONS: usize = 1024;
/// The default cap on concurrent HTTP/2 streams on one connection (the floor
/// RFC 9113 §6.5.2 recommends).
pub const DEFAULT_MAX_STREAMS_PER_CONNECTION: u32 = 100;

/// The HTTP-surface settings the router and listener are built from.
///
/// Kept apart from [`super::Config`] so the router (and the OpenAPI document
/// it describes) can be built without a database URL or signing seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpOptions {
    /// The largest `.crisp` upload accepted, in bytes.
    pub max_bundle_bytes: u64,
    /// The CORS origin allowlist. `None` means any origin is allowed (the
    /// lenient default); `Some` restricts cross-origin access to the listed
    /// origins.
    pub allowed_origins: Option<Vec<String>>,
    /// The per-operation abuse budgets (`CRISP_LIMIT_*`).
    pub limits: RateLimits,
    /// How many signature verifications and endorsements may run at once
    /// (`CRISP_VERIFY_CONCURRENCY`); see [`crate::state::CpuPool`].
    pub verify_concurrency: NonZeroUsize,
    /// Whose forwarding header names the client (`CRISP_TRUSTED_PROXIES`,
    /// `CRISP_PROXY_HEADER`).
    pub proxies: ProxyTrust,
    /// The listener's connection limits and timeouts.
    pub connections: ConnectionLimits,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            max_bundle_bytes: DEFAULT_MAX_BUNDLE_BYTES,
            allowed_origins: None,
            limits: RateLimits::default(),
            verify_concurrency: default_verify_concurrency(),
            proxies: ProxyTrust::default(),
            connections: ConnectionLimits::default(),
        }
    }
}

/// The default verification bound: one job per available CPU, so
/// verification alone can saturate the machine but never queue more threads
/// than it has cores to run them.
#[must_use]
pub fn default_verify_concurrency() -> NonZeroUsize {
    std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN)
}

/// Which peers may name the client in a forwarding header.
///
/// Forwarding headers are client-forgeable, so by default nobody is trusted
/// and the client is the TCP peer. Behind a reverse proxy, list the proxy's
/// address: the client is then the address that proxy wrote, and every
/// per-address rate limit counts real clients rather than the proxy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProxyTrust {
    /// The networks whose forwarding header is believed. Empty trusts nobody.
    pub networks: Vec<Network>,
    /// The header those proxies write.
    pub header: ProxyHeaderName,
}

impl ProxyTrust {
    /// The kynos trust policy these settings describe.
    #[must_use]
    pub fn policy(&self) -> TrustedProxies {
        if self.networks.is_empty() {
            return TrustedProxies::none();
        }
        let header = match self.header {
            ProxyHeaderName::XForwardedFor => ProxyHeader::XForwarded,
            ProxyHeaderName::Forwarded => ProxyHeader::Forwarded,
        };
        TrustedProxies::networks(header, self.networks.iter().copied())
    }
}

/// The forwarding header a trusted proxy writes (`CRISP_PROXY_HEADER`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProxyHeaderName {
    /// `X-Forwarded-For` (`x-forwarded-for`), what Caddy and most nginx
    /// configurations send. The default.
    #[default]
    XForwardedFor,
    /// RFC 7239 `Forwarded` (`forwarded`).
    Forwarded,
}

/// The limits the listener holds every connection to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionLimits {
    /// How long a client may take to send a request head, and how long an idle
    /// keep-alive connection is kept (`CRISP_HEADER_TIMEOUT_SECS`).
    pub header_timeout: Duration,
    /// How long a request body may go without a byte arriving before the
    /// request is answered 408 (`CRISP_BODY_IDLE_TIMEOUT_SECS`).
    pub body_idle_timeout: Duration,
    /// The most connections open at once; more wait in the accept backlog
    /// (`CRISP_MAX_CONNECTIONS`).
    pub max_connections: NonZeroUsize,
    /// The most concurrent HTTP/2 streams on one connection
    /// (`CRISP_MAX_STREAMS_PER_CONNECTION`). With `max_connections` it bounds
    /// the requests in flight at once.
    pub max_streams_per_connection: u32,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            header_timeout: Duration::from_secs(DEFAULT_HEADER_TIMEOUT_SECS),
            body_idle_timeout: Duration::from_secs(DEFAULT_BODY_IDLE_TIMEOUT_SECS),
            max_connections: NonZeroUsize::new(DEFAULT_MAX_CONNECTIONS)
                .unwrap_or(NonZeroUsize::MIN),
            max_streams_per_connection: DEFAULT_MAX_STREAMS_PER_CONNECTION,
        }
    }
}

/// Reads the connection limits through `set`, which yields a variable's value
/// only when it is set and non-blank.
pub(super) fn connection_limits(
    set: &impl Fn(&str) -> Option<String>,
) -> eyre::Result<ConnectionLimits> {
    let defaults = ConnectionLimits::default();
    let secs = |key: &str, default: Duration| -> eyre::Result<Duration> {
        Ok(Duration::from_secs(super::parse_positive(
            key,
            set(key),
            default.as_secs(),
        )?))
    };
    let max_connections = super::parse_positive(
        "CRISP_MAX_CONNECTIONS",
        set("CRISP_MAX_CONNECTIONS"),
        defaults.max_connections.get(),
    )?;
    Ok(ConnectionLimits {
        header_timeout: secs("CRISP_HEADER_TIMEOUT_SECS", defaults.header_timeout)?,
        body_idle_timeout: secs("CRISP_BODY_IDLE_TIMEOUT_SECS", defaults.body_idle_timeout)?,
        max_connections: NonZeroUsize::new(max_connections)
            .ok_or_else(|| eyre!("CRISP_MAX_CONNECTIONS must be positive"))?,
        max_streams_per_connection: super::parse_positive(
            "CRISP_MAX_STREAMS_PER_CONNECTION",
            set("CRISP_MAX_STREAMS_PER_CONNECTION"),
            defaults.max_streams_per_connection,
        )?,
    })
}

/// Reads the proxy trust settings through `set`.
///
/// `CRISP_TRUSTED_PROXIES` is a comma-separated list of `address/prefix`
/// networks or bare addresses (one host). `CRISP_PROXY_HEADER` is
/// `x-forwarded-for` (the default) or `forwarded`.
pub(super) fn proxy_trust(set: &impl Fn(&str) -> Option<String>) -> eyre::Result<ProxyTrust> {
    let header = match set("CRISP_PROXY_HEADER") {
        None => ProxyHeaderName::default(),
        Some(value) => match value.trim().to_ascii_lowercase().as_str() {
            "x-forwarded-for" => ProxyHeaderName::XForwardedFor,
            "forwarded" => ProxyHeaderName::Forwarded,
            _ => {
                return Err(eyre!(
                    "CRISP_PROXY_HEADER must be x-forwarded-for or forwarded, got {value:?}"
                ));
            }
        },
    };
    let networks = set("CRISP_TRUSTED_PROXIES")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(parse_network)
        .collect::<eyre::Result<_>>()?;
    Ok(ProxyTrust { networks, header })
}

/// Reads one trusted-proxy entry: `address/prefix`, or a bare address as the
/// network holding only that host.
fn parse_network(entry: &str) -> eyre::Result<Network> {
    let invalid = |reason: &dyn std::fmt::Display| {
        eyre!("CRISP_TRUSTED_PROXIES contains an invalid entry {entry:?}: {reason}")
    };
    if entry.contains('/') {
        return entry.parse().map_err(|reason| invalid(&reason));
    }
    let address: IpAddr = entry.parse().map_err(|reason| invalid(&reason))?;
    let prefix = if address.is_ipv4() { 32 } else { 128 };
    Network::new(address, prefix).map_err(|reason| invalid(&reason))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn read<T>(
        vars: &[(&str, &str)],
        parse: impl Fn(&dyn Fn(&str) -> Option<String>) -> eyre::Result<T>,
    ) -> eyre::Result<T> {
        let map: HashMap<&str, &str> = vars.iter().copied().collect();
        let set = |key: &str| {
            map.get(key)
                .map(|value| (*value).to_owned())
                .filter(|value| !value.trim().is_empty())
        };
        parse(&set)
    }

    fn limits(vars: &[(&str, &str)]) -> eyre::Result<ConnectionLimits> {
        read(vars, |set| connection_limits(&set))
    }

    fn trust(vars: &[(&str, &str)]) -> eyre::Result<ProxyTrust> {
        read(vars, |set| proxy_trust(&set))
    }

    #[test]
    fn connection_limits_default_when_unset() {
        let limits = limits(&[]).unwrap();
        assert_eq!(limits, ConnectionLimits::default());
        assert_eq!(limits.header_timeout, Duration::from_secs(30));
        assert_eq!(limits.body_idle_timeout, Duration::from_secs(30));
        assert_eq!(limits.max_connections.get(), 1024);
        assert_eq!(limits.max_streams_per_connection, 100);
    }

    #[test]
    fn connection_limits_are_read_from_the_environment() {
        let limits = limits(&[
            ("CRISP_HEADER_TIMEOUT_SECS", "5"),
            ("CRISP_BODY_IDLE_TIMEOUT_SECS", "7"),
            ("CRISP_MAX_CONNECTIONS", "64"),
            ("CRISP_MAX_STREAMS_PER_CONNECTION", "8"),
        ])
        .unwrap();
        assert_eq!(limits.header_timeout, Duration::from_secs(5));
        assert_eq!(limits.body_idle_timeout, Duration::from_secs(7));
        assert_eq!(limits.max_connections.get(), 64);
        assert_eq!(limits.max_streams_per_connection, 8);
    }

    #[test]
    fn malformed_connection_limits_are_rejected_not_defaulted() {
        for key in [
            "CRISP_HEADER_TIMEOUT_SECS",
            "CRISP_BODY_IDLE_TIMEOUT_SECS",
            "CRISP_MAX_CONNECTIONS",
            "CRISP_MAX_STREAMS_PER_CONNECTION",
        ] {
            for bad in ["abc", "-5", "0", "1.5"] {
                let error = limits(&[(key, bad)]).expect_err(&format!("{key}={bad}"));
                assert!(error.to_string().contains(key), "{error}");
            }
        }
    }

    #[test]
    fn nobody_is_trusted_by_default() {
        let trust = trust(&[]).unwrap();
        assert_eq!(trust, ProxyTrust::default());
        assert!(trust.policy().trusts_nobody());
    }

    #[test]
    fn trusted_proxies_accept_networks_and_bare_addresses() {
        let trust = trust(&[
            ("CRISP_TRUSTED_PROXIES", "172.16.0.0/12, 127.0.0.1 ,::1"),
            ("CRISP_PROXY_HEADER", "Forwarded"),
        ])
        .unwrap();
        let shown: Vec<String> = trust.networks.iter().map(ToString::to_string).collect();
        assert_eq!(shown, ["172.16.0.0/12", "127.0.0.1/32", "::1/128"]);
        assert_eq!(trust.header, ProxyHeaderName::Forwarded);
        assert_eq!(
            trust.policy().to_string(),
            "Forwarded from 172.16.0.0/12, 127.0.0.1/32, ::1/128"
        );
    }

    #[test]
    fn the_proxy_header_defaults_to_x_forwarded_for() {
        let trust = trust(&[("CRISP_TRUSTED_PROXIES", "10.0.0.1")]).unwrap();
        assert_eq!(trust.header, ProxyHeaderName::XForwardedFor);
        assert_eq!(
            trust.policy().to_string(),
            "X-Forwarded-For from 10.0.0.1/32"
        );
    }

    #[test]
    fn malformed_proxy_settings_are_rejected() {
        for bad in ["0.0.0.0/0", "10.0.0.0/33", "proxy.local", "10.0.0.0/"] {
            let error = trust(&[("CRISP_TRUSTED_PROXIES", bad)]).expect_err(bad);
            assert!(
                error.to_string().contains("CRISP_TRUSTED_PROXIES"),
                "{error}"
            );
        }
        let error = trust(&[("CRISP_PROXY_HEADER", "x-real-ip")]).unwrap_err();
        assert!(error.to_string().contains("CRISP_PROXY_HEADER"), "{error}");
    }
}
