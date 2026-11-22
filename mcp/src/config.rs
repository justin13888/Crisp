//! Process configuration, read once from the environment at start-up.
//!
//! Nothing here is required to start: a server with no key can still
//! validate drafts and read the library, and reports what is missing when a
//! tool needs it.

use std::path::PathBuf;

use secrecy::SecretString;

/// Where the identity key's passphrase comes from.
#[derive(Debug, Clone)]
pub enum PassphraseSource {
    /// A file whose contents (trailing newline trimmed) are the passphrase.
    File(PathBuf),
    /// The passphrase itself.
    Inline(SecretString),
}

/// The signing identity: an exported `.crispkey` file and its passphrase.
#[derive(Debug, Clone)]
pub struct KeyConfig {
    /// The passphrase-protected key file exported from the web app.
    pub key_file: PathBuf,
    /// Where the passphrase comes from.
    pub passphrase: PassphraseSource,
}

/// A Crisp server to publish to and search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// The server's base URL, e.g. `https://crisp.example`.
    pub url: String,
    /// The username the signing identity is registered under.
    pub username: String,
}

/// Everything `crisp-mcp` reads from its environment.
#[derive(Debug, Clone)]
pub struct Config {
    /// The directory `.crisp` files are stored in.
    pub library_dir: PathBuf,
    /// The signing identity, when configured.
    pub key: Option<KeyConfig>,
    /// The name recipes are signed under.
    pub display_name: Option<String>,
    /// The server to publish to, when configured.
    pub server: Option<ServerConfig>,
    /// Whether logs are JSON lines rather than text.
    pub json_logs: bool,
}

/// A configuration the environment describes inconsistently.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ConfigError(pub String);

impl Config {
    /// Reads the configuration from the process environment.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] when a variable is set without the one it
    /// needs (a key file with no passphrase, a server URL with no username),
    /// or when `CRISP_SERVER_URL` is not `https://` (`http://` is accepted
    /// only for `localhost`, `127.0.0.1`, and `[::1]`).
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok().filter(|v| !v.trim().is_empty()))
    }

    /// Reads the configuration through `lookup`, which returns a variable's
    /// non-empty value.
    ///
    /// # Errors
    ///
    /// See [`Config::from_env`].
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let library_dir = lookup("CRISP_LIBRARY_DIR")
            .map(PathBuf::from)
            .or_else(|| default_library_dir(&lookup))
            .ok_or_else(|| {
                ConfigError("set CRISP_LIBRARY_DIR (no HOME to default it from)".into())
            })?;
        let key = match lookup("CRISP_KEY_FILE") {
            None => None,
            Some(path) => {
                let passphrase = if let Some(file) = lookup("CRISP_KEY_PASSPHRASE_FILE") {
                    PassphraseSource::File(PathBuf::from(file))
                } else if let Some(inline) = lookup("CRISP_KEY_PASSPHRASE") {
                    PassphraseSource::Inline(SecretString::from(inline))
                } else {
                    return Err(ConfigError(
                        "CRISP_KEY_FILE is set: also set CRISP_KEY_PASSPHRASE_FILE \
                         (or CRISP_KEY_PASSPHRASE)"
                            .into(),
                    ));
                };
                Some(KeyConfig {
                    key_file: PathBuf::from(path),
                    passphrase,
                })
            }
        };
        let server = match (lookup("CRISP_SERVER_URL"), lookup("CRISP_SERVER_USERNAME")) {
            (None, _) => None,
            (Some(_), None) => {
                return Err(ConfigError(
                    "CRISP_SERVER_URL is set: also set CRISP_SERVER_USERNAME".into(),
                ));
            }
            (Some(url), Some(username)) => Some(ServerConfig {
                url: checked_server_url(&url)?,
                username,
            }),
        };
        Ok(Self {
            library_dir,
            key,
            display_name: lookup("CRISP_DISPLAY_NAME").map(|n| n.trim().to_owned()),
            server,
            json_logs: lookup("CRISP_LOG_FORMAT").is_some_and(|f| f == "json"),
        })
    }
}

/// Checks `CRISP_SERVER_URL` and returns it without its trailing slash.
///
/// Signing in sends a signed login and receives a bearer token, so the URL
/// must be `https://`, except `http://` to a loopback host (`localhost`,
/// `127.0.0.1`, `[::1]`), judged on the parsed host rather than the text.
fn checked_server_url(raw: &str) -> Result<String, ConfigError> {
    let parsed = url::Url::parse(raw)
        .map_err(|e| ConfigError(format!("CRISP_SERVER_URL `{raw}` is not a valid URL: {e}")))?;
    let Some(host) = parsed.host() else {
        return Err(ConfigError(format!(
            "CRISP_SERVER_URL `{raw}` has no host: use an https:// URL such as \
             https://crisp.example"
        )));
    };
    match parsed.scheme() {
        "https" => {}
        "http" if is_loopback(&host) => {}
        "http" => {
            return Err(ConfigError(format!(
                "CRISP_SERVER_URL `{raw}` uses http:// to a non-loopback host, so the \
                 sign-in and its session token would be sent unencrypted: use an \
                 https:// URL (http:// is accepted only for localhost, 127.0.0.1, and [::1])"
            )));
        }
        other => {
            return Err(ConfigError(format!(
                "CRISP_SERVER_URL `{raw}` uses the unsupported scheme `{other}`: use an \
                 https:// URL"
            )));
        }
    }
    tracing::debug!(scheme = parsed.scheme(), host = %host, "server URL accepted");
    Ok(raw.trim_end_matches('/').to_owned())
}

/// Whether `host` is exactly `localhost`, `127.0.0.1`, or `::1`.
fn is_loopback(host: &url::Host<&str>) -> bool {
    match host {
        url::Host::Domain(domain) => *domain == "localhost",
        url::Host::Ipv4(ip) => *ip == std::net::Ipv4Addr::LOCALHOST,
        url::Host::Ipv6(ip) => *ip == std::net::Ipv6Addr::LOCALHOST,
    }
}

/// `$XDG_DATA_HOME/crisp/library`, else `$HOME/.local/share/crisp/library`.
fn default_library_dir(lookup: &impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let data = lookup("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| lookup("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
    Some(data.join("crisp").join("library"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config(vars: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Config::from_lookup(|name| vars.get(name).cloned())
    }

    #[test]
    fn the_library_defaults_under_xdg_data_home_then_home() {
        let xdg = config(&[("XDG_DATA_HOME", "/x"), ("HOME", "/h")]).expect("config");
        assert_eq!(xdg.library_dir, PathBuf::from("/x/crisp/library"));
        let home = config(&[("HOME", "/h")]).expect("config");
        assert_eq!(
            home.library_dir,
            PathBuf::from("/h/.local/share/crisp/library")
        );
        let explicit = config(&[("HOME", "/h"), ("CRISP_LIBRARY_DIR", "/lib")]).expect("config");
        assert_eq!(explicit.library_dir, PathBuf::from("/lib"));
        assert!(config(&[]).is_err());
    }

    #[test]
    fn a_key_file_needs_a_passphrase_and_prefers_the_file() {
        assert!(config(&[("HOME", "/h"), ("CRISP_KEY_FILE", "/k")]).is_err());
        let both = config(&[
            ("HOME", "/h"),
            ("CRISP_KEY_FILE", "/k"),
            ("CRISP_KEY_PASSPHRASE", "inline"),
            ("CRISP_KEY_PASSPHRASE_FILE", "/p"),
        ])
        .expect("config");
        let key = both.key.expect("a key");
        assert_eq!(key.key_file, PathBuf::from("/k"));
        assert!(matches!(key.passphrase, PassphraseSource::File(p) if p.as_os_str() == "/p"));
        let inline = config(&[
            ("HOME", "/h"),
            ("CRISP_KEY_FILE", "/k"),
            ("CRISP_KEY_PASSPHRASE", "inline"),
        ])
        .expect("config");
        assert!(matches!(
            inline.key.expect("a key").passphrase,
            PassphraseSource::Inline(_)
        ));
    }

    #[test]
    fn a_server_needs_a_username_and_loses_its_trailing_slash() {
        assert!(config(&[("HOME", "/h"), ("CRISP_SERVER_URL", "https://s")]).is_err());
        let ok = config(&[
            ("HOME", "/h"),
            ("CRISP_SERVER_URL", "https://s/"),
            ("CRISP_SERVER_USERNAME", "me"),
        ])
        .expect("config");
        assert_eq!(
            ok.server,
            Some(ServerConfig {
                url: "https://s".into(),
                username: "me".into()
            })
        );
        assert!(config(&[("HOME", "/h")]).expect("config").server.is_none());
    }

    fn server_url(url: &str) -> Result<Config, ConfigError> {
        config(&[
            ("HOME", "/h"),
            ("CRISP_SERVER_URL", url),
            ("CRISP_SERVER_USERNAME", "me"),
        ])
    }

    #[test]
    fn https_and_http_to_a_loopback_host_are_accepted() {
        for (url, stored) in [
            ("https://crisp.example/", "https://crisp.example"),
            (
                "https://crisp.example:8443/api",
                "https://crisp.example:8443/api",
            ),
            ("http://localhost", "http://localhost"),
            ("http://localhost:3000/", "http://localhost:3000"),
            ("http://127.0.0.1:8080/crisp", "http://127.0.0.1:8080/crisp"),
            ("http://[::1]:8080", "http://[::1]:8080"),
        ] {
            let server = server_url(url)
                .unwrap_or_else(|e| panic!("{url} refused: {e}"))
                .server
                .expect("a server");
            assert_eq!(server.url, stored, "{url}");
        }
    }

    #[test]
    fn http_to_any_other_host_is_refused_by_its_parsed_host() {
        for url in [
            "http://crisp.example",
            "http://localhost.evil.example",
            "http://127.0.0.1.evil.example",
            "http://localhost@evil.example",
            "http://127.0.0.2",
            "http://[::2]",
            "http://10.0.0.1:8080",
        ] {
            let ConfigError(message) = server_url(url).expect_err(url);
            assert!(message.contains("CRISP_SERVER_URL"), "{message}");
            assert!(message.contains("unencrypted"), "{message}");
            assert!(message.contains("https://"), "{message}");
        }
    }

    #[test]
    fn other_schemes_unparsable_urls_and_missing_hosts_are_refused() {
        for url in [
            "ftp://crisp.example",
            "ws://localhost",
            "file:///tmp/crisp",
            "crisp.example",
            "https://",
            "not a url",
        ] {
            let ConfigError(message) = server_url(url).expect_err(url);
            assert!(message.contains("CRISP_SERVER_URL"), "{message}");
        }
    }

    #[test]
    fn display_name_and_log_format_are_read() {
        let c = config(&[
            ("HOME", "/h"),
            ("CRISP_DISPLAY_NAME", "  Ada  "),
            ("CRISP_LOG_FORMAT", "json"),
        ])
        .expect("config");
        assert_eq!(c.display_name.as_deref(), Some("Ada"));
        assert!(c.json_logs);
        assert!(!config(&[("HOME", "/h")]).expect("config").json_logs);
    }
}
