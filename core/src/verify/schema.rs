//! B1 — the schema rules `serde` cannot express.
//!
//! Decoding into the model already enforces field names, types, and the
//! closed schema. This module adds the rest of `crisp-v1.cddl`: no `null`
//! anywhere, NFC text, fixed byte lengths, minimum counts, and the syntax of
//! language tags, unit codes, slugs, vocabulary codes, and extension keys.

use ciborium::value::Value;
use unicode_normalization::is_nfc;

use crate::error::ErrorCode;
use crate::limits as l;
use crate::model::meta::PublicKeySet;
use crate::model::payload::{BundlePayload, ExtensionMap};
use crate::model::primitives::{LanguageTag, Quantity, Rational};
use crate::model::recipe::{Identity, Recipe};
use crate::model::text::{LocalizedProse, LocalizedText};
use crate::verify::issue::Issues;
use crate::verify::walk::{self, QuantityKind, SlugKind, TextKind, Visitor, pointer, walk_value};

const S: ErrorCode = ErrorCode::Schema;

/// Checks the raw decoded payload `value`: no `null`, every text NFC.
pub(crate) fn check_value(value: &Value) -> Issues {
    let mut issues = Issues::default();
    walk_value("", value, &mut |path, value| match value {
        Value::Null => issues.push(path, S, "null is not permitted in a Crisp payload"),
        Value::Text(text) if !is_nfc(text) => {
            issues.push(path, S, "text is not Unicode NFC");
        }
        _ => {}
    });
    issues
}

/// Whether `len` is a non-empty byte length of at most `max`.
fn sized(len: usize, max: usize) -> bool {
    (1..=max).contains(&len)
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
        issues.require(
            sized(author.display_name.len(), l::MAX_DISPLAY_NAME_BYTES),
            &format!("{path}/display_name"),
            S,
            || {
                format!(
                    "display name must be 1..{} bytes",
                    l::MAX_DISPLAY_NAME_BYTES
                )
            },
        );
        keyset_rules(&author.keyset, &format!("{path}/keyset"), issues);
    }
    issues.require(
        sized(meta.license.len(), l::MAX_LICENSE_BYTES),
        "/meta/license",
        S,
        || format!("license must be 1..{} bytes", l::MAX_LICENSE_BYTES),
    );

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
        identity.author_fpr.len() == l::FINGERPRINT_BYTES,
        &format!("{path}/author_fpr"),
        S,
        || format!("author_fpr must be {} bytes", l::FINGERPRINT_BYTES),
    );
    issues.require(
        identity.birth_nonce.len() == l::BIRTH_NONCE_BYTES,
        &format!("{path}/birth_nonce"),
        S,
        || format!("birth_nonce must be {} bytes", l::BIRTH_NONCE_BYTES),
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
        keyset.ed25519.len() == l::ED25519_PUBLIC_KEY_BYTES,
        &format!("{path}/ed25519"),
        S,
        || {
            format!(
                "an Ed25519 public key is {} bytes",
                l::ED25519_PUBLIC_KEY_BYTES
            )
        },
    );
    issues.require(
        keyset.ml_dsa_65.len() == l::ML_DSA_65_PUBLIC_KEY_BYTES,
        &format!("{path}/ml-dsa-65"),
        S,
        || {
            format!(
                "an ML-DSA-65 public key is {} bytes",
                l::ML_DSA_65_PUBLIC_KEY_BYTES
            )
        },
    );
}

fn recipe_rules(recipe: &Recipe, rp: &str, issues: &mut Issues) {
    issues.require(!recipe.steps.is_empty(), &format!("{rp}/steps"), S, || {
        "a recipe needs at least one step".to_owned()
    });
    if let Some(source) = &recipe.source {
        if let Some(title) = &source.title {
            issues.require(
                sized(title.len(), l::MAX_TITLE_BYTES),
                &format!("{rp}/source/title"),
                S,
                || format!("source title must be 1..{} bytes", l::MAX_TITLE_BYTES),
            );
        }
        if let Some(author) = &source.author {
            issues.require(
                sized(author.len(), l::MAX_SOURCE_AUTHOR_BYTES),
                &format!("{rp}/source/author"),
                S,
                || {
                    format!(
                        "source author must be 1..{} bytes",
                        l::MAX_SOURCE_AUTHOR_BYTES
                    )
                },
            );
        }
    }
}

/// Whether `value` is 1..64 bytes of `[a-z0-9]` runs joined by single `sep`.
pub(crate) fn is_token(value: &str, sep: char) -> bool {
    sized(value.len(), l::MAX_SLUG_BYTES)
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
    if !(l::LANGUAGE_TAG_MIN_BYTES..=l::LANGUAGE_TAG_MAX_BYTES).contains(&tag.len()) {
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
    (l::EXT_KEY_MIN_BYTES..=l::EXT_KEY_MAX_BYTES).contains(&key.len())
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
            self.language(&pointer(path, &key.0), key);
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
                .require(!value.is_empty(), &pointer(path, &lang.0), S, || {
                    "text must not be empty".to_owned()
                });
        }
    }

    fn recipe(&mut self, path: &str, _index: usize, recipe: &'a Recipe) {
        recipe_rules(recipe, path, &mut self.issues);
    }

    fn identity(&mut self, path: &str, identity: &'a Identity) {
        identity_rules(identity, path, &mut self.issues);
    }

    fn slug(&mut self, path: &str, slug: &'a str, kind: SlugKind) {
        match kind {
            SlugKind::Tag | SlugKind::Code => {
                self.issues.require(is_token(slug, '-'), path, S, || {
                    format!("{slug:?} is not a lowercase slug (a-z, 0-9, single hyphens)")
                });
            }
            SlugKind::Nutrient => {
                self.issues.require(is_token(slug, '_'), path, S, || {
                    format!("nutrient code {slug:?} is not a snake_case token")
                });
            }
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
                (l::UNIT_CODE_MIN_BYTES..=l::UNIT_CODE_MAX_BYTES).contains(&unit.0.len()),
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
                .require(is_ext_key(key), &pointer(path, key), S, || {
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

    #[test]
    fn value_rules_escape_map_keys_and_see_through_tags() {
        let value = Value::Map(vec![(
            Value::Text("a/b~c".to_owned()),
            Value::Tag(0, Box::new(Value::Null)),
        )]);
        let paths: Vec<String> = check_value(&value).0.into_iter().map(|i| i.path).collect();
        assert_eq!(paths, ["/a~1b~0c"]);
    }

    #[test]
    fn walk_reported_rules_land_at_their_paths() {
        use crate::fixtures::{amount, sample_payload};
        use crate::model::primitives::OpenCode;
        use crate::model::recipe::Nutrition;

        let mut payload = sample_payload();
        let recipe = &mut payload.recipes[0];
        recipe.identity.author_fpr = vec![0; l::FINGERPRINT_BYTES + 1];
        recipe.tags = Some(vec!["ok".to_owned(), "Not Ok".to_owned()]);
        recipe.diet = Some(vec![OpenCode::Other("a--b".to_owned())]);
        recipe.nutrition = Some(Nutrition {
            basis: amount(1, "g"),
            values: [(OpenCode::Other("x/y".to_owned()), amount(1, "g"))]
                .into_iter()
                .collect(),
        });
        recipe.ext = Some(ExtensionMap::from([(
            "bad/key".to_owned(),
            Value::Bool(true),
        )]));
        let paths: Vec<String> = check_payload(&payload)
            .0
            .into_iter()
            .map(|i| i.path)
            .collect();
        assert_eq!(
            paths,
            [
                "/recipes/0/identity/author_fpr",
                "/recipes/0/nutrition/values/x~1y",
                "/recipes/0/tags/1",
                "/recipes/0/diet/0",
                "/recipes/0/ext/bad~1key",
            ]
        );
    }
}
