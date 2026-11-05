//! `CrispServer`: the Crisp server API for JavaScript, built on the
//! spargen-generated client. Signing happens in [`crate::identity`]; this
//! module only moves bytes and typed JSON.

use crisp_core::verify::PlatformKey;
use secrecy::SecretString;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;

use crate::api;
use crate::api::{self as support, types};
use crate::boundary::{encode_hex, failure, from_js, to_js};
use crate::format::version_text;

mod problem;
mod views;

use problem::server_failure;
pub use views::{
    AccountUsageView, Challenge, RemotePage, RemoteRecipe, SearchQuery, ServerInfoView, Session,
};
use views::{joined, page, platform_key};

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
                .map_err(|e| server_failure!("reading server info", &e))?
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
                .map_err(|e| server_failure!("registering", &e))?;
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
                .map_err(|e| server_failure!("requesting a login challenge", &e))?
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
                .map_err(|e| server_failure!("signing in", &e))?
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
                .map_err(|e| server_failure!("signing out", &e))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Ends every session of the signed-in account, on every device,
    /// including this one.
    #[wasm_bindgen(js_name = logoutAll, unchecked_return_type = "Promise<void>")]
    pub fn logout_all(&self) -> js_sys::Promise {
        let client = self.client();
        future_to_promise(async move {
            client?
                .logout_all()
                .await
                .map_err(|e| server_failure!("signing out everywhere", &e))?;
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
                .map_err(|e| server_failure!("publishing", &e))?;
            Ok(js_sys::Uint8Array::from(response.into_inner().as_ref()).into())
        })
    }

    /// Reads how much the signed-in account stores against its quotas.
    #[wasm_bindgen(unchecked_return_type = "Promise<AccountUsageView>")]
    pub fn usage(&self) -> js_sys::Promise {
        let client = self.client();
        future_to_promise(async move {
            let usage = client?
                .account_usage()
                .await
                .map_err(|e| server_failure!("reading account usage", &e))?
                .into_inner();
            to_js(&AccountUsageView::from(usage))
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
                Some(revision) => problem::found(client.get_exact(doc_id, revision).await)
                    .map_err(|e| server_failure!("downloading", &e))?,
                None => problem::found(client.get_latest(doc_id).await)
                    .map_err(|e| server_failure!("downloading", &e))?,
            };
            Ok(bytes.map_or(JsValue::NULL, |b| {
                js_sys::Uint8Array::from(b.as_ref()).into()
            }))
        })
    }

    /// Removes what the signed-in account published of a document: exactly
    /// `revision`, or every revision it published when `revision` is
    /// `null`. Revisions co-authors published stay. Rejects with code
    /// `forbidden` (403) when the account published none of them, and with
    /// status 404 when there is nothing to remove.
    #[wasm_bindgen(unchecked_return_type = "Promise<void>")]
    pub fn unpublish(&self, doc_id: &str, revision: Option<String>) -> js_sys::Promise {
        let client = self.client();
        let doc_id = doc_id.to_owned();
        future_to_promise(async move {
            let client = client?;
            match revision {
                Some(revision) => client
                    .unpublish_revision(doc_id, revision)
                    .await
                    .map(drop)
                    .map_err(|e| server_failure!("unpublishing", &e))?,
                None => client
                    .unpublish_document(doc_id)
                    .await
                    .map(drop)
                    .map_err(|e| server_failure!("unpublishing", &e))?,
            }
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Deletes the signed-in account and everything it published. The
    /// session must be fresh: a session older than ten minutes rejects with
    /// code `reauthentication-required` (403), and signing in again with the
    /// key file clears it.
    #[wasm_bindgen(js_name = deleteAccount, unchecked_return_type = "Promise<void>")]
    pub fn delete_account(&self) -> js_sys::Promise {
        let client = self.client();
        future_to_promise(async move {
            client?
                .delete_account()
                .await
                .map_err(|e| server_failure!("deleting the account", &e))?;
            Ok(JsValue::UNDEFINED)
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
                .map_err(|e| server_failure!("browsing", &e))?;
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
                .map_err(|e| server_failure!("searching", &e))?;
            to_js(&page(response.into_inner()))
        })
    }
}
