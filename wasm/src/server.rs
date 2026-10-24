//! `CrispServer`: the Crisp server API for JavaScript, built on the
//! spargen-generated client. Signing happens in [`crate::identity`]; this
//! module only moves bytes and typed JSON.

use crisp_core::model::meta::PublicKeySet;
use crisp_core::model::primitives::Timestamp;
use crisp_core::verify::PlatformKey;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

use crate::api;
use crate::api::{self as support, types};
use crate::boundary::{decode_hex, encode_hex, failure, from_js, to_js};
use crate::format::version_text;

/// Extracts the RFC 9457 `detail` of a documented error body.
trait Detail {
    /// The problem detail, if the body carries one.
    fn detail(&self) -> Option<String>;
}

fn detail_of<T: Serialize>(body: &T) -> Option<String> {
    serde_json::to_value(body)
        .ok()?
        .get("detail")?
        .as_str()
        .map(str::to_owned)
}

impl Detail for std::convert::Infallible {
    fn detail(&self) -> Option<String> {
        match *self {}
    }
}

macro_rules! details {
    ($($error:ident { $($variant:ident),* $(,)? }),* $(,)?) => {
        $(impl Detail for api::$error {
            fn detail(&self) -> Option<String> {
                match self {
                    $(api::$error::$variant(body) => detail_of(body),)*
                }
            }
        })*
    };
}

details! {
    AccountProfileError { Status400, Status404, Status500 },
    ChallengeError { Status400, Status413, Status415, Status422, Status500 },
    LoginError { Status400, Status401, Status413, Status415, Status422, Status500 },
    LogoutError { Status401, Status403, Status500 },
    RegisterError { Status400, Status409, Status413, Status415, Status422, Status500 },
    PublishError { Status400, Status401, Status403, Status409, Status413, Status415, Status422, Status500 },
    GetLatestError { Status400, Status404, Status500 },
    GetExactError { Status400, Status404, Status500 },
    BrowseError { Status400, Status500 },
    SearchError { Status400, Status500 },
}

/// Converts a client error into a JS failure with `status` and the
/// server's problem `detail`.
fn server_failure<E: Detail + std::fmt::Debug + std::fmt::Display>(
    what: &str,
    error: &support::Error<E>,
) -> JsValue {
    let status = error.status().map(|s| s.as_u16());
    let detail = match error {
        support::Error::Api(value) => value.inner().detail(),
        _ => None,
    };
    let message = match (detail, status) {
        (Some(detail), _) => detail,
        (None, Some(status)) => format!("{what} failed with HTTP {status}"),
        (None, None) if error.is_transient() => {
            format!("{what} failed: the server could not be reached")
        }
        (None, None) => format!("{what} failed: {error}"),
    };
    failure(&message, None, status)
}

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

fn page(page: types::RecipePage) -> RemotePage {
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

fn platform_key(info: &types::PlatformKeyInfo) -> Option<PlatformKey> {
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

fn joined(values: &[String]) -> Option<String> {
    (!values.is_empty()).then(|| values.join(","))
}

/// A Crisp server at one base URL, with an optional session.
#[wasm_bindgen]
pub struct CrispServer {
    base_url: String,
    token: Option<String>,
}

impl CrispServer {
    /// A generated client, carrying the session token when there is one.
    fn client(&self) -> Result<api::Client, JsValue> {
        let client = api::Client::new(&self.base_url)
            .map_err(|e| failure(&format!("invalid server URL: {e}"), None, None))?;
        Ok(match &self.token {
            Some(token) => client.with_credential(
                "sessionToken",
                support::Credential::Bearer(SecretString::from(token.clone())),
            ),
            None => client,
        })
    }
}

#[wasm_bindgen]
impl CrispServer {
    /// A server at `baseUrl` (e.g. `https://crisp.example`), signed out.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(base_url: &str) -> CrispServer {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: None,
        }
    }

    /// Uses `token` (or no session, for `null`) on later calls.
    #[wasm_bindgen(js_name = setToken)]
    pub fn set_token(&mut self, token: Option<String>) {
        self.token = token;
    }

    /// Reads the server's versions and endorsement keys.
    #[wasm_bindgen(unchecked_return_type = "Promise<ServerInfoView>")]
    pub fn info(&self) -> js_sys::Promise {
        let client = self.client();
        future_to_promise(async move {
            let info = client?
                .server_info()
                .await
                .map_err(|e| server_failure("reading server info", &e))?
                .into_inner();
            let text = |v: i64| version_text(u64::try_from(v).unwrap_or(0));
            let platform_keys: Vec<PlatformKey> =
                info.platform_keys.iter().filter_map(platform_key).collect();
            let platform_user_id = info
                .platform_keys
                .first()
                .map(|k| k.usr_id.clone())
                .ok_or_else(|| failure("the server publishes no endorsement key", None, None))?;
            to_js(&ServerInfoView {
                api_versions: info.api_versions,
                core_version: info.core_version,
                format_version_text: text(info.format_version.current),
                min_format_version_text: text(info.format_version.min_supported),
                platform_keys,
                platform_user_id,
            })
        })
    }

    /// Registers an account from `Signer.signProfile` bytes; resolves to
    /// the account's `usr…` id.
    #[wasm_bindgen(unchecked_return_type = "Promise<string>")]
    pub fn register(&self, signed_profile: &[u8]) -> js_sys::Promise {
        let client = self.client();
        let profile = encode_hex(signed_profile);
        future_to_promise(async move {
            let response = client?
                .register(&types::RegisterRequest { profile })
                .await
                .map_err(|e| server_failure("registering", &e))?;
            Ok(response.into_inner().usr_id.into())
        })
    }

    /// Requests a login challenge for `username`.
    #[wasm_bindgen(unchecked_return_type = "Promise<Challenge>")]
    pub fn challenge(&self, username: &str) -> js_sys::Promise {
        let client = self.client();
        let username = username.to_owned();
        future_to_promise(async move {
            let response = client?
                .challenge(&types::ChallengeRequest { username })
                .await
                .map_err(|e| server_failure("requesting a login challenge", &e))?
                .into_inner();
            to_js(&Challenge {
                nonce: response.nonce,
                audience: response.audience,
            })
        })
    }

    /// Exchanges a signed challenge for a session.
    #[wasm_bindgen(unchecked_return_type = "Promise<Session>")]
    pub fn login(&self, nonce: &str, signature: &[u8]) -> js_sys::Promise {
        let client = self.client();
        let request = types::LoginRequest {
            nonce: nonce.to_owned(),
            signature: encode_hex(signature),
        };
        future_to_promise(async move {
            let response = client?
                .login(&request)
                .await
                .map_err(|e| server_failure("signing in", &e))?
                .into_inner();
            to_js(&Session {
                user_id: response.usr_id,
                token: response.token,
                expires_at: response.expires_at,
            })
        })
    }

    /// Ends the current session.
    #[wasm_bindgen(unchecked_return_type = "Promise<void>")]
    pub fn logout(&self) -> js_sys::Promise {
        let client = self.client();
        future_to_promise(async move {
            client?
                .logout()
                .await
                .map_err(|e| server_failure("signing out", &e))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Publishes an author-signed `.crisp` file; resolves to the endorsed
    /// file bytes.
    #[wasm_bindgen(unchecked_return_type = "Promise<Uint8Array>")]
    pub fn publish(&self, bundle: &[u8]) -> js_sys::Promise {
        let client = self.client();
        let body = bytes::Bytes::copy_from_slice(bundle);
        future_to_promise(async move {
            let response = client?
                .publish(&body)
                .await
                .map_err(|e| server_failure("publishing", &e))?;
            Ok(js_sys::Uint8Array::from(response.into_inner().as_ref()).into())
        })
    }

    /// Downloads a published file — the latest revision, or exactly
    /// `revision`. Resolves to `null` if there is none.
    #[wasm_bindgen(unchecked_return_type = "Promise<Uint8Array | null>")]
    pub fn download(&self, doc_id: &str, revision: Option<String>) -> js_sys::Promise {
        let client = self.client();
        let doc_id = doc_id.to_owned();
        future_to_promise(async move {
            let client = client?;
            let bytes = match revision {
                Some(revision) => match client.get_exact(doc_id, revision).await {
                    Ok(r) => Some(r.into_inner()),
                    Err(e) if e.status().is_some_and(|s| s.as_u16() == 404) => None,
                    Err(e) => return Err(server_failure("downloading", &e)),
                },
                None => match client.get_latest(doc_id).await {
                    Ok(r) => Some(r.into_inner()),
                    Err(e) if e.status().is_some_and(|s| s.as_u16() == 404) => None,
                    Err(e) => return Err(server_failure("downloading", &e)),
                },
            };
            Ok(bytes.map_or(JsValue::NULL, |b| {
                js_sys::Uint8Array::from(b.as_ref()).into()
            }))
        })
    }

    /// Lists published recipes, newest first.
    #[wasm_bindgen(unchecked_return_type = "Promise<RemotePage>")]
    pub fn browse(&self, limit: u32, offset: u32) -> js_sys::Promise {
        let client = self.client();
        future_to_promise(async move {
            let params = api::BrowseParams::default()
                .limit(i64::from(limit))
                .offset(i64::from(offset));
            let response = client?
                .browse(params)
                .await
                .map_err(|e| server_failure("browsing", &e))?;
            to_js(&page(response.into_inner()))
        })
    }

    /// Searches published recipes.
    #[wasm_bindgen(unchecked_return_type = "Promise<RemotePage>")]
    pub fn search(
        &self,
        #[wasm_bindgen(unchecked_param_type = "SearchQuery")] query: JsValue,
    ) -> js_sys::Promise {
        let client = self.client();
        let query: Result<SearchQuery, JsValue> = from_js(query, "search query");
        future_to_promise(async move {
            let query = query?;
            let params = api::SearchParams {
                q: query.q.filter(|q| !q.trim().is_empty()),
                cuisine: joined(&query.cuisine),
                course: joined(&query.course),
                diet: joined(&query.diet),
                allergen_free: joined(&query.allergen_free),
                language: query.language,
                limit: query.limit,
                offset: query.offset,
            };
            let response = client?
                .search(params)
                .await
                .map_err(|e| server_failure("searching", &e))?;
            to_js(&page(response.into_inner()))
        })
    }
}
