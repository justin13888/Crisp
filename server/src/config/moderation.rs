//! The moderation settings: who administers the server (`CRISP_ADMIN_USR_IDS`)
//! and where its abuse contact and terms live (`CRISP_ABUSE_CONTACT`,
//! `CRISP_TERMS_URL`).

use std::collections::BTreeSet;

use eyre::eyre;

/// Who may call the `/admin` routes, and what `GET /server-info` tells
/// readers about reporting abuse and the operator's terms.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModerationOptions {
    /// The `usr…` ids of the administrator accounts (`CRISP_ADMIN_USR_IDS`,
    /// comma-separated; empty by default, so no account administers).
    pub admins: BTreeSet<String>,
    /// Where rights holders and readers report abuse, e.g. a `mailto:` or
    /// web address (`CRISP_ABUSE_CONTACT`).
    pub abuse_contact: Option<String>,
    /// The operator's terms of service (`CRISP_TERMS_URL`, an `http(s)` URL).
    pub terms_url: Option<String>,
}

impl ModerationOptions {
    /// Whether `usr_id` is a listed administrator.
    #[must_use]
    pub fn is_admin(&self, usr_id: &str) -> bool {
        self.admins.contains(usr_id)
    }

    /// Logs the administrator count at INFO and a WARN for each of the abuse
    /// contact and the terms that is unset: a public server needs both.
    pub fn log_startup(&self) {
        tracing::info!(admins = self.admins.len(), "moderation administrators");
        if self.abuse_contact.is_none() {
            tracing::warn!(
                "CRISP_ABUSE_CONTACT is unset; GET /server-info advertises no abuse contact"
            );
        }
        if self.terms_url.is_none() {
            tracing::warn!("CRISP_TERMS_URL is unset; GET /server-info advertises no terms");
        }
    }
}

/// Reads the moderation settings through `set`.
///
/// # Errors
///
/// Returns an error if a listed administrator is not a `usr…` id or the
/// terms URL is not an `http(s)` URL.
pub(super) fn moderation(set: &impl Fn(&str) -> Option<String>) -> eyre::Result<ModerationOptions> {
    let admins = set("CRISP_ADMIN_USR_IDS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(|id| {
            let well_formed = id.len() > 3
                && id.starts_with("usr")
                && id.chars().all(|c| c.is_ascii_alphanumeric());
            if well_formed {
                Ok(id.to_owned())
            } else {
                Err(eyre!(
                    "CRISP_ADMIN_USR_IDS contains {id:?}, which is not a usr… account id"
                ))
            }
        })
        .collect::<eyre::Result<_>>()?;
    let terms_url = set("CRISP_TERMS_URL").map(|url| url.trim().to_owned());
    if let Some(url) = &terms_url {
        let host = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"));
        if !host.is_some_and(|host| !host.is_empty() && !host.starts_with('/')) {
            return Err(eyre!("CRISP_TERMS_URL must be an http(s) URL, got {url:?}"));
        }
    }
    Ok(ModerationOptions {
        admins,
        abuse_contact: set("CRISP_ABUSE_CONTACT").map(|contact| contact.trim().to_owned()),
        terms_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(vars: &[(&str, &str)]) -> eyre::Result<ModerationOptions> {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        moderation(&|key: &str| {
            vars.iter()
                .find(|(k, v)| k == key && !v.trim().is_empty())
                .map(|(_, v)| v.clone())
        })
    }

    #[test]
    fn nothing_is_set_by_default() {
        let options = read(&[]).unwrap();
        assert_eq!(options, ModerationOptions::default());
        assert!(!options.is_admin("usr1a"));
    }

    #[test]
    fn administrators_contact_and_terms_are_read() {
        let options = read(&[
            ("CRISP_ADMIN_USR_IDS", " usr1a, usr1b ,,"),
            ("CRISP_ABUSE_CONTACT", " mailto:abuse@crisp.example "),
            ("CRISP_TERMS_URL", "https://crisp.example/terms"),
        ])
        .unwrap();
        assert!(options.is_admin("usr1a") && options.is_admin("usr1b"));
        assert!(!options.is_admin("usr1c"));
        assert_eq!(options.admins.len(), 2);
        assert_eq!(
            options.abuse_contact.as_deref(),
            Some("mailto:abuse@crisp.example")
        );
        assert_eq!(
            options.terms_url.as_deref(),
            Some("https://crisp.example/terms")
        );
    }

    #[test]
    fn malformed_administrators_are_rejected() {
        for bad in ["rcp1a", "usr", "usr1a;drop", "usr1a, x"] {
            let error = read(&[("CRISP_ADMIN_USR_IDS", bad)]).expect_err(bad);
            assert!(error.to_string().contains("CRISP_ADMIN_USR_IDS"), "{error}");
        }
    }

    #[test]
    fn malformed_terms_urls_are_rejected() {
        for bad in [
            "crisp.example/terms",
            "ftp://x",
            "https://",
            "http:///terms",
        ] {
            let error = read(&[("CRISP_TERMS_URL", bad)]).expect_err(bad);
            assert!(error.to_string().contains("CRISP_TERMS_URL"), "{error}");
        }
    }
}
