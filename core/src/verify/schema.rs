//! B1 — the schema rules `serde` cannot express.
//!
//! Decoding into the model already enforces field names, types, and the
//! closed schema. This module adds the rest of `crisp-v1.cddl`: no `null`
//! anywhere, NFC text, fixed byte lengths, minimum counts, and the syntax of
//! language tags, unit codes, slugs, vocabulary codes, and extension keys.

use ciborium::value::Value;
use unicode_normalization::is_nfc;

use crate::error::ErrorCode;
use crate::limits;
use crate::model::meta::PublicKeySet;
use crate::model::payload::{BundlePayload, ExtensionMap};
use crate::model::primitives::{LanguageTag, OpenCode, Quantity, Rational};
use crate::model::recipe::{Identity, Recipe};
use crate::model::text::{LocalizedProse, LocalizedText};
use crate::verify::issue::Issues;
use crate::verify::walk::{self, QuantityKind, TextKind, Visitor};

const S: ErrorCode = ErrorCode::Schema;

/// Checks the raw decoded payload `value`: no `null`, every text NFC.
pub(crate) fn check_value(value: &Value) -> Issues {
    let mut issues = Issues::default();
    value_rules(value, "", &mut issues);
    issues
}

fn value_rules(value: &Value, path: &str, issues: &mut Issues) {
    match value {
        Value::Null => issues.push(path, S, "null is not permitted in a Crisp payload"),
        Value::Text(text) if !is_nfc(text) => {
            issues.push(path, S, "text is not Unicode NFC");
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                value_rules(item, &format!("{path}/{i}"), issues);
            }
        }
        Value::Map(entries) => {
            for (key, item) in entries {
                let segment = match key {
                    Value::Text(k) => escape(k),
                    Value::Integer(n) => i128::from(*n).to_string(),
                    _ => "?".to_owned(),
                };
                let child = format!("{path}/{segment}");
                value_rules(key, &child, issues);
                value_rules(item, &child, issues);
            }
        }
        Value::Tag(_, inner) => value_rules(inner, path, issues),
        _ => {}
    }
}

/// Escapes a JSON Pointer reference token (RFC 6901 §3).
fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// Checks the typed payload against the CDDL rules `serde` cannot express.
pub(crate) fn check_payload(payload: &BundlePayload) -> Issues {
    let mut rules = Rules::default();
    walk::walk(payload, &mut rules);
    let issues = &mut rules.issues;

    issues.require(!payload.languages.is_empty(), "/languages", S, || {
        "at least one language is required".to_owned()
    });
    issues.require(!payload.recipes.is_empty(), "/recipes", S, || {
        "at least one recipe is required".to_owned()
    });

    let meta = &payload.meta;
    issues.require(!meta.authors.is_empty(), "/meta/authors", S, || {
        "at least one author is required".to_owned()
    });
    for (i, author) in meta.authors.iter().enumerate() {
        let path = format!("/meta/authors/{i}");
        let name = author.display_name.len();
        issues.require(
            (1..=256).contains(&name),
            &format!("{path}/display_name"),
            S,
            || "display name must be 1..256 bytes".to_owned(),
        );
        keyset_rules(&author.keyset, &format!("{path}/keyset"), issues);
    }
    issues.require(
        (1..=64).contains(&meta.license.len()),
        "/meta/license",
        S,
        || "license must be 1..64 bytes".to_owned(),
    );
    if let Some(identity) = &meta.identity {
        identity_rules(identity, "/meta/identity", issues);
    }

    for (i, recipe) in payload.recipes.iter().enumerate() {
        recipe_rules(recipe, &format!("/recipes/{i}"), issues);
    }

    if let Some(book) = &payload.book {
        issues.require(!book.parts.is_empty(), "/book/parts", S, || {
            "a book needs at least one part".to_owned()
        });
        for (pi, part) in book.parts.iter().enumerate() {
            let pp = format!("/book/parts/{pi}");
            issues.require(
                !part.chapters.is_empty(),
                &format!("{pp}/chapters"),
                S,
                || "a part needs at least one chapter".to_owned(),
            );
            for (ci, chapter) in part.chapters.iter().enumerate() {
                issues.require(
                    !chapter.entries.is_empty(),
                    &format!("{pp}/chapters/{ci}/entries"),
                    S,
                    || "a chapter needs at least one entry".to_owned(),
                );
            }
        }
    }
    rules.issues
}

fn identity_rules(identity: &Identity, path: &str, issues: &mut Issues) {
    issues.require(
        identity.author_fpr.len() == 32,
        &format!("{path}/author_fpr"),
        S,
        || "author_fpr must be 32 bytes".to_owned(),
    );
    issues.require(
        identity.birth_nonce.len() == 16,
        &format!("{path}/birth_nonce"),
        S,
        || "birth_nonce must be 16 bytes".to_owned(),
    );
    issues.require(
        !identity.title.is_empty(),
        &format!("{path}/title"),
        S,
        || "the identity title must not be empty".to_owned(),
    );
}

fn keyset_rules(keyset: &PublicKeySet, path: &str, issues: &mut Issues) {
    issues.require(
        keyset.ed25519.len() == 32,
        &format!("{path}/ed25519"),
        S,
        || "an Ed25519 public key is 32 bytes".to_owned(),
    );
    issues.require(
        keyset.ml_dsa_65.len() == 1952,
        &format!("{path}/ml-dsa-65"),
        S,
        || "an ML-DSA-65 public key is 1952 bytes".to_owned(),
    );
}

fn recipe_rules(recipe: &Recipe, rp: &str, issues: &mut Issues) {
    identity_rules(&recipe.identity, &format!("{rp}/identity"), issues);
    issues.require(!recipe.steps.is_empty(), &format!("{rp}/steps"), S, || {
        "a recipe needs at least one step".to_owned()
    });
    for (field, values) in [("tags", &recipe.tags), ("cuisine", &recipe.cuisine)] {
        for (i, value) in values.iter().flatten().enumerate() {
            slug_rule(value, &format!("{rp}/{field}/{i}"), issues);
        }
    }
    open_codes(recipe.course.as_deref(), &format!("{rp}/course"), issues);
    open_codes(recipe.diet.as_deref(), &format!("{rp}/diet"), issues);
    open_codes(
        recipe.allergens.as_deref(),
        &format!("{rp}/allergens"),
        issues,
    );
    if let Some(nutrition) = &recipe.nutrition {
        for code in nutrition.values.keys() {
            if let OpenCode::Other(code) = code {
                issues.require(
                    is_token(code, '_'),
                    &format!("{rp}/nutrition/values/{code}"),
                    S,
                    || format!("nutrient code {code:?} is not a snake_case token"),
                );
            }
        }
    }
    if let Some(source) = &recipe.source {
        if let Some(title) = &source.title {
            issues.require(
                (1..=512).contains(&title.len()),
                &format!("{rp}/source/title"),
                S,
                || "source title must be 1..512 bytes".to_owned(),
            );
        }
        if let Some(author) = &source.author {
            issues.require(
                (1..=256).contains(&author.len()),
                &format!("{rp}/source/author"),
                S,
                || "source author must be 1..256 bytes".to_owned(),
            );
        }
    }
}

fn open_codes<T>(codes: Option<&[OpenCode<T>]>, path: &str, issues: &mut Issues) {
    for (i, code) in codes.into_iter().flatten().enumerate() {
        if let OpenCode::Other(code) = code {
            slug_rule(code, &format!("{path}/{i}"), issues);
        }
    }
}

fn slug_rule(value: &str, path: &str, issues: &mut Issues) {
    issues.require(is_token(value, '-'), path, S, || {
        format!("{value:?} is not a lowercase slug (a-z, 0-9, single hyphens)")
    });
}

/// Whether `value` is 1..64 bytes of `[a-z0-9]` runs joined by single `sep`.
pub(crate) fn is_token(value: &str, sep: char) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.split(sep).all(|run| {
            !run.is_empty()
                && run
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

/// Whether `tag` is a syntactically valid BCP-47 tag within the CDDL size:
/// a 2–3 letter primary subtag, then 1–8 character alphanumeric subtags.
pub(crate) fn is_language_tag(tag: &str) -> bool {
    if !(limits::LANGUAGE_TAG_MIN_BYTES..=limits::LANGUAGE_TAG_MAX_BYTES).contains(&tag.len()) {
        return false;
    }
    let mut subtags = tag.split('-');
    let primary_ok = subtags
        .next()
        .is_some_and(|p| (2..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphabetic()));
    primary_ok
        && subtags
            .all(|s| (1..=8).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Whether `key` is a reverse-DNS extension key, e.g. `com.example.feature`.
fn is_ext_key(key: &str) -> bool {
    (3..=128).contains(&key.len())
        && key.split('.').count() >= 2
        && key.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// The walk-driven part of B1.
#[derive(Default)]
struct Rules {
    issues: Issues,
}

impl Rules {
    fn localized_keys<'k>(&mut self, path: &str, keys: impl Iterator<Item = &'k LanguageTag>) {
        let mut any = false;
        for key in keys {
            any = true;
            self.language(&format!("{path}/{}", key.0), key);
        }
        self.issues.require(any, path, S, || {
            "a localized value needs at least one language".to_owned()
        });
    }
}

impl<'a> Visitor<'a> for Rules {
    fn text(&mut self, path: &str, text: &'a LocalizedText, _kind: TextKind) {
        self.localized_keys(path, text.0.keys());
        for (lang, value) in &text.0 {
            self.issues
                .require(!value.is_empty(), &format!("{path}/{}", lang.0), S, || {
                    "text must not be empty".to_owned()
                });
        }
    }

    fn prose(&mut self, path: &str, prose: &'a LocalizedProse, _scope: walk::ProseScope<'a>) {
        self.localized_keys(path, prose.0.keys());
    }

    fn quantity(&mut self, path: &str, quantity: &'a Quantity, _kind: QuantityKind) {
        for (name, value) in [("min", &quantity.min), ("max", &quantity.max)] {
            if let Some(value) = value {
                self.rational(&format!("{path}/{name}"), value);
            }
        }
        if let Some(unit) = &quantity.unit {
            self.issues.require(
                (limits::UNIT_CODE_MIN_BYTES..=limits::UNIT_CODE_MAX_BYTES).contains(&unit.0.len()),
                &format!("{path}/unit"),
                S,
                || "a unit code must be 1..64 bytes".to_owned(),
            );
        }
    }

    fn rational(&mut self, path: &str, value: &'a Rational) {
        self.issues.require(value.denominator >= 1, path, S, || {
            "a rational's denominator must be at least 1".to_owned()
        });
    }

    fn ext(&mut self, path: &str, ext: &'a ExtensionMap) {
        for key in ext.keys() {
            self.issues
                .require(is_ext_key(key), &format!("{path}/{key}"), S, || {
                    format!("extension key {key:?} is not reverse-DNS namespaced")
                });
        }
    }

    fn language(&mut self, path: &str, tag: &'a LanguageTag) {
        self.issues.require(is_language_tag(&tag.0), path, S, || {
            format!("{:?} is not a valid BCP-47 language tag", tag.0)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_slugs_and_tags() {
        assert!(is_token("gluten-free", '-'));
        assert!(!is_token("Gluten", '-'));
        assert!(!is_token("a--b", '-'));
        assert!(is_token("energy_kcal", '_'));
        assert!(is_language_tag("en"));
        assert!(is_language_tag("zh-Hant-TW"));
        assert!(!is_language_tag("e"));
        assert!(!is_language_tag("english"));
        assert!(is_ext_key("com.example.feature"));
        assert!(!is_ext_key("feature"));
        assert!(!is_ext_key("Com.Example"));
    }

    #[test]
    fn value_rules_flag_null_and_non_nfc_with_paths() {
        let value = Value::Map(vec![
            (Value::Text("a".to_owned()), Value::Null),
            (
                Value::Text("b".to_owned()),
                Value::Text("e\u{301}".to_owned()),
            ),
        ]);
        let issues = check_value(&value).0;
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].path, "/a");
        assert_eq!(issues[1].path, "/b");
    }
}
