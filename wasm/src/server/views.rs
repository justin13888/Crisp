//! The typed values `CrispServer` hands JavaScript, and their conversion
//! from the generated client's response types.

use crisp_core::model::meta::PublicKeySet;
use crisp_core::model::primitives::Timestamp;
use crisp_core::verify::PlatformKey;
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::api::types;
use crate::boundary::decode_hex;

/// What the client needs from `GET /server-info`.
#[derive(Serialize, Deserialize, Tsify)]
pub struct ServerInfoView {
    /// The API versions the server speaks.
    pub api_versions: Vec<String>,
    /// The server's crisp-core version.
    pub core_version: String,
    /// The format version the server writes, as `major.minor.patch`.
    pub format_version_text: String,
    /// The lowest format version it reads, as `major.minor.patch`.
    pub min_format_version_text: String,
    /// The trusted endorsement keys, ready for `Bundle.verify`.
    pub platform_keys: Vec<PlatformKey>,
    /// The platform key's `usr…` id — the login audience.
    pub platform_user_id: String,
}

/// A login challenge to answer with `Signer.signLogin`.
#[derive(Serialize, Deserialize, Tsify)]
pub struct Challenge {
    /// The hex nonce.
    pub nonce: String,
    /// The platform `usr…` id the answer is bound to.
    pub audience: String,
}

/// An authenticated session.
#[derive(Serialize, Deserialize, Tsify)]
pub struct Session {
    /// The account's `usr…` id.
    pub user_id: String,
    /// The bearer token.
    pub token: String,
    /// When the token expires (epoch seconds).
    pub expires_at: i64,
}

/// How much the signed-in account stores on the server, against its quotas.
#[derive(Serialize, Deserialize, Tsify)]
pub struct AccountUsageView {
    /// The total endorsed size of the account's bundles, in bytes.
    pub bytes_used: i64,
    /// The most bytes the account may store.
    pub bytes_limit: i64,
    /// How many bundles (revisions) the account has published.
    pub bundles_used: i64,
    /// The most bundles the account may store.
    pub bundles_limit: i64,
}

impl From<types::AccountUsage> for AccountUsageView {
    fn from(usage: types::AccountUsage) -> Self {
        Self {
            bytes_used: usage.bytes_used,
            bytes_limit: usage.bytes_limit,
            bundles_used: usage.bundles_used,
            bundles_limit: usage.bundles_limit,
        }
    }
}

/// One published recipe, as listed by browse and search.
#[derive(Serialize, Deserialize, Tsify)]
pub struct RemoteRecipe {
    /// The bundle's document id.
    pub doc_id: String,
    /// The recipe id.
    pub recipe_id: String,
    /// The revision id.
    pub revision: String,
    /// The title in the bundle's default language.
    pub title: String,
    /// A plain-text summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Author display names.
    pub authors: Vec<String>,
    /// Tags, cuisine, course, and diet codes.
    pub tags: Vec<String>,
    /// Declared allergens.
    pub allergens: Vec<String>,
    /// The identity language.
    pub language: String,
    /// When last modified (epoch seconds).
    pub modified_at: i64,
}

/// A page of published recipes.
#[derive(Serialize, Deserialize, Tsify)]
pub struct RemotePage {
    /// The recipes.
    pub recipes: Vec<RemoteRecipe>,
    /// The page size asked for.
    pub limit: i64,
    /// The offset asked for.
    pub offset: i64,
}

/// A search over published recipes. Lists are matched as "any of".
#[derive(Serialize, Deserialize, Tsify, Default)]
#[serde(default)]
pub struct SearchQuery {
    /// Free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    /// Cuisine slugs.
    pub cuisine: Vec<String>,
    /// Course codes.
    pub course: Vec<String>,
    /// Diet codes.
    pub diet: Vec<String>,
    /// Allergens the recipe must not declare.
    pub allergen_free: Vec<String>,
    /// An identity language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The page size (1–200).
    pub limit: Option<i64>,
    /// The offset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
}

pub(super) fn page(page: types::RecipePage) -> RemotePage {
    RemotePage {
        limit: page.limit,
        offset: page.offset,
        recipes: page
            .recipes
            .into_iter()
            .map(|r| {
                let mut tags = r.course;
                tags.extend(r.cuisine);
                tags.extend(r.diet);
                tags.extend(r.tags);
                RemoteRecipe {
                    doc_id: r.doc_id,
                    recipe_id: r.recipe_id,
                    revision: r.recipe_revision,
                    title: r.title,
                    summary: r.summary,
                    authors: r.authors.into_iter().map(|a| a.name).collect(),
                    tags,
                    allergens: r.allergens,
                    language: r.language,
                    modified_at: r.modified_at,
                }
            })
            .collect(),
    }
}

pub(super) fn platform_key(info: &types::PlatformKeyInfo) -> Option<PlatformKey> {
    Some(PlatformKey {
        keyset: PublicKeySet {
            ed25519: decode_hex(&info.ed25519)?,
            ml_dsa_65: decode_hex(&info.ml_dsa_65)?,
        },
        valid_from: Timestamp(u64::try_from(info.valid_from.unwrap_or(0)).ok()?),
        valid_until: info
            .valid_until
            .and_then(|t| u64::try_from(t).ok())
            .map(Timestamp),
    })
}

pub(super) fn joined(values: &[String]) -> Option<String> {
    (!values.is_empty()).then(|| values.join(","))
}

#[cfg(test)]
mod tests {
    use super::{joined, platform_key};
    use crisp_core::model::meta::PublicKeySet;
    use crisp_core::model::primitives::Timestamp;
    use crisp_core::verify::PlatformKey;

    fn info(fields: serde_json::Value) -> crate::api::types::PlatformKeyInfo {
        serde_json::from_value(fields).expect("platform key info")
    }

    #[test]
    fn a_platform_key_decodes_its_keys_and_window() {
        let key = info(serde_json::json!({
            "ed25519": "0a0b",
            "ml_dsa_65": "ff",
            "usr_id": "usrplatform",
            "valid_from": 10,
            "valid_until": 20,
        }));
        assert_eq!(
            platform_key(&key),
            Some(PlatformKey {
                keyset: PublicKeySet {
                    ed25519: vec![0x0a, 0x0b],
                    ml_dsa_65: vec![0xff],
                },
                valid_from: Timestamp(10),
                valid_until: Some(Timestamp(20)),
            })
        );
    }

    #[test]
    fn a_platform_key_with_bad_hex_is_dropped() {
        let key = info(serde_json::json!({
            "ed25519": "zz",
            "ml_dsa_65": "ff",
            "usr_id": "usrplatform",
        }));
        assert_eq!(platform_key(&key), None);
    }

    #[test]
    fn filter_values_join_with_commas_and_none_is_no_filter() {
        assert_eq!(joined(&[]), None);
        assert_eq!(joined(&["a".to_owned()]), Some("a".to_owned()));
        assert_eq!(
            joined(&["a".to_owned(), "b".to_owned()]),
            Some("a,b".to_owned())
        );
    }
}
