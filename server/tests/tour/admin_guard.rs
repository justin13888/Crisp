//! Every operation demanding the `admin` scope resolves a failed session
//! lookup before acting on it: the authorizer lets such a lookup through
//! unjudged, so this is what keeps a non-administrator out when the
//! database fails mid-authentication.

use crisp_server::db::{moderation_action, report, suspension, takedown};
use crisp_server::error::code;
use kynos::http::StatusCode;

use crate::common::{Fault, Harness, assert_code};

/// The moderation state an `/admin` handler could change.
fn moderation_state(
    h: &Harness,
) -> (
    Vec<report::Model>,
    Vec<takedown::Model>,
    Vec<suspension::Model>,
    Vec<moderation_action::Model>,
) {
    let inner = h.storage.inner.lock().expect("storage lock");
    (
        inner.reports.clone(),
        inner.takedowns.clone(),
        inner.suspensions.clone(),
        inner.actions.clone(),
    )
}

/// A session lookup the database interrupted reaches the admin authorizer
/// unjudged, and it lets the request through so the handler can answer the
/// failure (`SessionLookup` in `src/auth/bearer.rs`). Every `/admin` route is
/// safe only because its handler resolves the lookup before anything else:
/// here a *non-administrator's* token, with every other storage call
/// healthy, must still be answered 500 on each one, and leave the moderation
/// state untouched. The `valid_session` pass is the one that proves it for
/// every route: under the `account_by_usr_id` fault, suspend and unsuspend
/// could fail on a lookup of their own. The requests come from
/// [`ADMIN_CALLS`], which is checked against every operation the API
/// publishes as demanding the `admin` scope, so a new one fails here until
/// it is driven too.
pub async fn admin_lookup_failures(h: &Harness, token: &str) {
    h.storage
        .inner
        .lock()
        .expect("storage lock")
        .reports
        .push(report::Model {
            id: 1,
            reporter: None,
            doc_id: "doc1guard".to_owned(),
            revision: None,
            category: "spam".to_owned(),
            detail: String::new(),
            status: "open".to_owned(),
            created_at: 0,
            resolved_at: None,
            resolved_by: None,
        });
    let before = moderation_state(h);
    let c = &h.client;
    for op in ["valid_session", "account_by_usr_id"] {
        h.storage.inject(op, Fault::Db);
        for call in ADMIN_CALLS {
            let request = match call.method {
                "get" => c.get(call.path),
                "post" => c.post(call.path),
                "delete" => c.delete(call.path),
                other => panic!("add a `{other}` arm to drive {}", call.template),
            };
            let request = match &call.body {
                Some(body) => request
                    .json(&serde_json::from_str::<serde_json::Value>(body).expect("a JSON body")),
                None => request,
            };
            let response = request.bearer(token).send().await;
            response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
            assert_code(&response, code::INTERNAL);
        }
        h.storage.heal();
    }
    assert!(
        moderation_state(h) == before,
        "an /admin handler acted on a session lookup that failed"
    );
    let mut driven: Vec<(String, String)> = ADMIN_CALLS
        .iter()
        .map(|call| (call.method.to_owned(), call.template.to_owned()))
        .collect();
    driven.sort();
    assert_eq!(
        admin_operations(),
        driven,
        "every operation demanding the admin scope must be in ADMIN_CALLS"
    );
}

/// One request [`admin_lookup_failures`] sends to an admin-scoped operation.
struct AdminCall {
    /// The operation's method, as the OpenAPI document keys it.
    method: &'static str,
    /// The operation's path template, as the OpenAPI document keys it.
    template: &'static str,
    /// The concrete path sent.
    path: &'static str,
    /// The JSON body sent, if any.
    body: Option<&'static str>,
}

/// Every request [`admin_lookup_failures`] sends: the one list both the
/// requests and the coverage check are read from.
const ADMIN_CALLS: &[AdminCall] = &[
    AdminCall {
        method: "get",
        template: "/api/v1/admin/reports",
        path: "/api/v1/admin/reports",
        body: None,
    },
    AdminCall {
        method: "post",
        template: "/api/v1/admin/reports/{id}/resolve",
        path: "/api/v1/admin/reports/1/resolve",
        body: Some(r#"{"resolution":"actioned"}"#),
    },
    AdminCall {
        method: "post",
        template: "/api/v1/admin/takedowns",
        path: "/api/v1/admin/takedowns",
        body: Some(r#"{"doc_id":"doc1guard","reason":"spam"}"#),
    },
    AdminCall {
        method: "delete",
        template: "/api/v1/admin/takedowns/{doc_id}",
        path: "/api/v1/admin/takedowns/doc1guard",
        body: None,
    },
    AdminCall {
        method: "post",
        template: "/api/v1/admin/accounts/{usr_id}/suspend",
        path: "/api/v1/admin/accounts/usr1guard/suspend",
        body: Some(r#"{"reason":"spam"}"#),
    },
    AdminCall {
        method: "post",
        template: "/api/v1/admin/accounts/{usr_id}/unsuspend",
        path: "/api/v1/admin/accounts/usr1guard/unsuspend",
        body: None,
    },
];

/// Every operation in the committed OpenAPI document (which
/// `tests/openapi.rs` holds equal to the router's) that demands the `admin`
/// scope of `sessionToken`, whatever its path or method, as
/// `(method, path)`, sorted.
fn admin_operations() -> Vec<(String, String)> {
    /// The Path Item fields that are not operations.
    const NOT_OPERATIONS: &[&str] = &["parameters", "summary", "description", "servers", "$ref"];
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../spec/openapi.json");
    let document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("reads the document"))
            .expect("the document is JSON");
    let demands_admin = |operation: &serde_json::Value| {
        operation["security"]
            .as_array()
            .is_some_and(|requirements| {
                requirements.iter().any(|requirement| {
                    requirement["sessionToken"]
                        .as_array()
                        .is_some_and(|scopes| scopes.iter().any(|scope| scope == "admin"))
                })
            })
    };
    let mut operations: Vec<(String, String)> = document["paths"]
        .as_object()
        .expect("the document has paths")
        .iter()
        .flat_map(|(path, item)| {
            item.as_object()
                .expect("a path item is an object")
                .iter()
                .filter(|(key, operation)| {
                    !NOT_OPERATIONS.contains(&key.as_str()) && demands_admin(operation)
                })
                .map(|(method, _)| (method.clone(), path.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    operations.sort();
    operations
}
