//! Phase D — policy (spec §11.1, D1–D4).

use std::collections::BTreeSet;

use crate::error::ErrorCode;
use crate::model::payload::BundlePayload;
use crate::model::primitives::{LanguageTag, Quantity, Rational};
use crate::model::recipe::Recipe;
use crate::model::text::{LocalizedProse, LocalizedText};
use crate::render::units::{UnitKind, lookup};
use crate::verify::issue::Issues;
use crate::verify::walk::{self, ProseScope, QuantityKind, TextKind, Visitor, pointer};

/// Whether `url` is a well-formed `https` URL with a plain host: no
/// credentials, whitespace, control characters, or backslashes.
#[must_use]
pub fn is_https_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    if url
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return false;
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !host.is_empty()
        && !host.contains('@')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
}

/// D1 — every link uses the `https` scheme and is well formed.
pub(crate) fn links(payload: &BundlePayload) -> Issues {
    struct Links(Issues);
    impl<'a> Visitor<'a> for Links {
        fn prose(&mut self, path: &str, prose: &'a LocalizedProse, _scope: ProseScope<'a>) {
            for (lang, body) in &prose.0 {
                for (pi, paragraph) in body.0.iter().enumerate() {
                    for (ri, run) in paragraph.0.iter().enumerate() {
                        if let Some(link) = &run.link {
                            let lp = pointer(path, &lang.0);
                            self.url(&format!("{lp}/{pi}/{ri}/link"), link);
                        }
                    }
                }
            }
        }
        fn url(&mut self, path: &str, url: &'a str) {
            self.0
                .require(is_https_url(url), path, ErrorCode::UrlScheme, || {
                    format!("{url:?} is not a well-formed https URL")
                });
        }
    }
    let mut v = Links(Issues::default());
    walk::walk(payload, &mut v);
    v.0
}

/// D2 — localization is complete and every language is declared.
pub(crate) fn localization(payload: &BundlePayload) -> Issues {
    struct L10n<'a> {
        declared: BTreeSet<&'a LanguageTag>,
        default: &'a LanguageTag,
        issues: Issues,
    }
    impl<'a> L10n<'a> {
        fn keys<'k>(&mut self, path: &str, keys: impl Iterator<Item = &'k LanguageTag>) {
            let mut has_default = false;
            for key in keys {
                has_default |= key == self.default;
                self.issues.require(
                    self.declared.contains(key),
                    &pointer(path, &key.0),
                    ErrorCode::L10nUndeclared,
                    || format!("language {:?} is not declared in languages", key.0),
                );
            }
            self.issues
                .require(has_default, path, ErrorCode::L10nDefault, || {
                    format!("missing the default language {:?}", self.default.0)
                });
        }
    }
    impl<'a> Visitor<'a> for L10n<'a> {
        fn text(&mut self, path: &str, text: &'a LocalizedText, _kind: TextKind) {
            self.keys(path, text.0.keys());
        }
        fn prose(&mut self, path: &str, prose: &'a LocalizedProse, _scope: ProseScope<'a>) {
            self.keys(path, prose.0.keys());
        }
        fn quantity(&mut self, path: &str, quantity: &'a Quantity, _kind: QuantityKind) {
            if let Some(text) = &quantity.text {
                self.keys(&format!("{path}/text"), text.0.keys());
            }
        }
        fn language(&mut self, path: &str, tag: &'a LanguageTag) {
            if path.ends_with("identity/language") || path == "/default_language" {
                self.issues.require(
                    self.declared.contains(tag),
                    path,
                    ErrorCode::L10nUndeclared,
                    || format!("language {:?} is not declared in languages", tag.0),
                );
            }
        }
    }
    let mut v = L10n {
        declared: payload.languages.iter().collect(),
        default: &payload.default_language,
        issues: Issues::default(),
    };
    walk::walk(payload, &mut v);
    v.issues
}

/// D3 — `created_at` never follows `modified_at`.
pub(crate) fn time_order(payload: &BundlePayload) -> Issues {
    let mut issues = Issues::default();
    let meta = &payload.meta;
    issues.require(
        meta.created_at <= meta.modified_at,
        "/meta/modified_at",
        ErrorCode::TimeOrder,
        || "meta.created_at is after meta.modified_at".to_owned(),
    );
    for (i, recipe) in payload.recipes.iter().enumerate() {
        issues.require(
            recipe.identity.created_at <= recipe.modified_at,
            &format!("/recipes/{i}/modified_at"),
            ErrorCode::TimeOrder,
            || "the recipe was modified before it was created".to_owned(),
        );
    }
    issues
}

/// Compares two rationals exactly.
fn le(a: Rational, b: Rational) -> bool {
    i128::from(a.numerator) * i128::from(b.denominator)
        <= i128::from(b.numerator) * i128::from(a.denominator)
}

/// D4 — every quantity is meaningful: a numeric `min` or a `text`, ranges in
/// order, durations in time units, temperature ranges in order.
pub(crate) fn quantities(payload: &BundlePayload) -> Issues {
    struct Q(Issues);
    impl<'a> Visitor<'a> for Q {
        fn quantity(&mut self, path: &str, q: &'a Quantity, kind: QuantityKind) {
            self.0.require(
                q.min.is_some() || q.text.is_some(),
                path,
                ErrorCode::Quantity,
                || "a quantity needs a numeric min or a text amount".to_owned(),
            );
            if let Some(max) = q.max {
                let ordered = q
                    .min
                    .is_some_and(|min| min.denominator > 0 && max.denominator > 0 && le(min, max));
                self.0
                    .require(ordered, &format!("{path}/max"), ErrorCode::Quantity, || {
                        "a range needs a min that does not exceed its max".to_owned()
                    });
            }
            if kind == QuantityKind::Duration && q.min.is_some() {
                let timed = q
                    .unit
                    .as_ref()
                    .and_then(|u| lookup(&u.0))
                    .is_some_and(|u| u.kind == UnitKind::Time);
                self.0
                    .require(timed, &format!("{path}/unit"), ErrorCode::Quantity, || {
                        "a duration needs a time unit (s, min, h, d)".to_owned()
                    });
            }
        }
        fn recipe(&mut self, path: &str, _index: usize, recipe: &'a Recipe) {
            for (i, step) in recipe.steps.iter().enumerate() {
                if let Some(t) = &step.temperature
                    && let Some(max) = t.max
                {
                    let ordered = t.min.denominator > 0 && max.denominator > 0 && le(t.min, max);
                    self.0.require(
                        ordered,
                        &format!("{path}/steps/{i}/temperature/max"),
                        ErrorCode::Quantity,
                        || "a temperature range needs min <= max".to_owned(),
                    );
                }
            }
        }
    }
    let mut v = Q(Issues::default());
    walk::walk(payload, &mut v);
    v.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_urls_are_validated_strictly() {
        assert!(is_https_url("https://example.com"));
        assert!(is_https_url("https://example.com:8443/a/b?c=d#e"));
        assert!(!is_https_url("http://example.com"));
        assert!(!is_https_url("https://"));
        assert!(!is_https_url("https://user@evil.com"));
        assert!(!is_https_url("https://exa mple.com"));
        assert!(!is_https_url("javascript:alert(1)"));
    }

    #[test]
    fn rationals_compare_exactly() {
        assert!(le(Rational::new(1, 3), Rational::new(1, 2)));
        assert!(!le(Rational::new(2, 3), Rational::new(1, 2)));
        assert!(le(Rational::new(-1, 1), Rational::new(0, 1)));
    }
}
