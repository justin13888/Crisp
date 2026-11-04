//! The observability settings: where metrics are served
//! (`CRISP_METRICS_BIND_ADDR`) and how log lines are written
//! (`CRISP_LOG_FORMAT`).

use std::str::FromStr;

use eyre::eyre;

/// The default metrics listen address: loopback only, on a port of its own,
/// so the public listener and the reverse proxy in front of it never serve
/// metrics.
pub const DEFAULT_METRICS_BIND_ADDR: &str = "127.0.0.1:9464";

/// The `CRISP_METRICS_BIND_ADDR` value that turns the metrics listener off.
pub const METRICS_OFF: &str = "off";

/// How log lines are written to stdout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    /// Human-readable `tracing` lines (the default).
    #[default]
    Text,
    /// One JSON object per event, with its span context, for a log store.
    Json,
}

impl FromStr for LogFormat {
    type Err = eyre::Report;

    fn from_str(value: &str) -> eyre::Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(eyre!(
                "CRISP_LOG_FORMAT must be `text` or `json`, got {value:?}"
            )),
        }
    }
}

impl LogFormat {
    /// Reads `CRISP_LOG_FORMAT` through `lookup`; unset or blank is
    /// [`LogFormat::Text`]. `main` reads it from the process environment
    /// before anything else, since the subscriber must be installed before
    /// the first line is logged.
    ///
    /// # Errors
    ///
    /// Returns an error if the variable is set to anything but `text` or
    /// `json`.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> eyre::Result<Self> {
        lookup("CRISP_LOG_FORMAT")
            .filter(|value| !value.trim().is_empty())
            .map_or(Ok(Self::Text), |value| value.parse())
    }
}

/// Where operational metrics are served, and how logs are written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservabilityOptions {
    /// The `host:port` the metrics listener binds to, or `None` when it is
    /// off (`CRISP_METRICS_BIND_ADDR`, `off` to disable).
    pub metrics_bind_addr: Option<String>,
    /// The log line format (`CRISP_LOG_FORMAT`).
    pub log_format: LogFormat,
}

impl Default for ObservabilityOptions {
    fn default() -> Self {
        Self {
            metrics_bind_addr: Some(DEFAULT_METRICS_BIND_ADDR.to_owned()),
            log_format: LogFormat::Text,
        }
    }
}

/// Reads the observability settings through `set`, which yields only
/// non-blank values.
///
/// # Errors
///
/// Returns an error if `CRISP_LOG_FORMAT` is malformed, or if
/// `CRISP_METRICS_BIND_ADDR` equals `CRISP_BIND_ADDR`, which would put the
/// metrics on the public listener's address.
pub(super) fn observability(
    set: &impl Fn(&str) -> Option<String>,
    bind_addr: &str,
) -> eyre::Result<ObservabilityOptions> {
    let metrics_bind_addr = match set("CRISP_METRICS_BIND_ADDR") {
        None => Some(DEFAULT_METRICS_BIND_ADDR.to_owned()),
        Some(value) if value.trim().eq_ignore_ascii_case(METRICS_OFF) => None,
        Some(value) => Some(value.trim().to_owned()),
    };
    if metrics_bind_addr.as_deref() == Some(bind_addr) {
        return Err(eyre!(
            "CRISP_METRICS_BIND_ADDR ({bind_addr}) must differ from CRISP_BIND_ADDR: \
             metrics are never served on the public listener"
        ));
    }
    Ok(ObservabilityOptions {
        metrics_bind_addr,
        log_format: LogFormat::from_lookup(set)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(vars: &[(&str, &str)], bind_addr: &str) -> eyre::Result<ObservabilityOptions> {
        let vars = vars.to_vec();
        observability(
            &move |key: &str| {
                vars.iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| (*v).to_owned())
            },
            bind_addr,
        )
    }

    #[test]
    fn metrics_default_to_a_loopback_listener_and_logs_to_text() {
        let options = read(&[], "0.0.0.0:8080").unwrap();
        assert_eq!(options, ObservabilityOptions::default());
        assert_eq!(
            options.metrics_bind_addr.as_deref(),
            Some(DEFAULT_METRICS_BIND_ADDR)
        );
        assert_eq!(options.log_format, LogFormat::Text);
    }

    #[test]
    fn metrics_bind_address_is_read_and_can_be_turned_off() {
        let options = read(&[("CRISP_METRICS_BIND_ADDR", "0.0.0.0:9100")], "x").unwrap();
        assert_eq!(options.metrics_bind_addr.as_deref(), Some("0.0.0.0:9100"));
        for off in ["off", "OFF", " Off "] {
            let options = read(&[("CRISP_METRICS_BIND_ADDR", off)], "x").unwrap();
            assert_eq!(options.metrics_bind_addr, None, "{off:?}");
        }
    }

    #[test]
    fn metrics_never_share_the_public_address() {
        let error = read(
            &[("CRISP_METRICS_BIND_ADDR", "0.0.0.0:8080")],
            "0.0.0.0:8080",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("must differ"), "{error}");
    }

    #[test]
    fn log_format_is_parsed_case_insensitively_and_rejects_unknowns() {
        let json = read(&[("CRISP_LOG_FORMAT", "JSON")], "x").unwrap();
        assert_eq!(json.log_format, LogFormat::Json);
        let text = read(&[("CRISP_LOG_FORMAT", "text")], "x").unwrap();
        assert_eq!(text.log_format, LogFormat::Text);
        let error = read(&[("CRISP_LOG_FORMAT", "yaml")], "x")
            .unwrap_err()
            .to_string();
        assert!(error.contains("CRISP_LOG_FORMAT"), "{error}");
    }

    #[test]
    fn a_blank_log_format_is_the_default() {
        let lookup = |key: &str| (key == "CRISP_LOG_FORMAT").then(|| "  ".to_owned());
        assert_eq!(LogFormat::from_lookup(lookup).unwrap(), LogFormat::Text);
    }
}
