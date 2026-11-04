//! Session management over the real router: logging out everywhere ends
//! every session of the caller's account and no other, and the session list
//! shows the caller's live sessions without ever revealing a token.

mod common;
mod tour;

use common::{Harness, now};
use crisp_core::sign::SecretKeySet;
use crisp_server::auth::sessions::{SESSION_ID_LEN, SessionList};
use kynos::http::StatusCode;
use tour::sessions::{list_sessions, logout_all, second_session};

#[tokio::test]
async fn logout_all_ends_every_session_of_the_caller_and_no_other() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[100u8; 32]);
    let bystander = SecretKeySet::from_seed(&[101u8; 32]);
    let phone = h.register_and_login(&author, "everywhere").await;
    let laptop = second_session(&h, &author, "everywhere").await;
    let other = h.register_and_login(&bystander, "bystander").await;

    let response = logout_all(&h, &laptop).await;
    response.assert_status(StatusCode::NO_CONTENT);
    assert!(response.bytes().is_empty());

    // The other device's token stops authenticating, and so does the
    // token that made the request.
    for token in [&phone, &laptop] {
        assert!(!h.storage.has_session(token));
        list_sessions(&h, token)
            .await
            .assert_status(StatusCode::UNAUTHORIZED);
    }
    // Another account's session is untouched.
    assert!(h.storage.has_session(&other));
    list_sessions(&h, &other)
        .await
        .assert_status(StatusCode::OK);
    h.client.assert_conformance();
}

#[tokio::test]
async fn logout_all_also_removes_expired_sessions() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[102u8; 32]);
    let usr_id = h.register(&author, "lapsed").await;
    h.storage.put_session("expired-token", &usr_id, now() - 1);
    h.storage.put_session("live-token", &usr_id, now() + 60);

    logout_all(&h, "live-token")
        .await
        .assert_status(StatusCode::NO_CONTENT);
    assert!(h.storage.inner.lock().unwrap().sessions.is_empty());
}

#[tokio::test]
async fn sessions_lists_live_sessions_newest_first_and_marks_the_caller() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[103u8; 32]);
    let bystander = SecretKeySet::from_seed(&[104u8; 32]);
    let usr_id = h.register(&author, "lister").await;
    let other_id = h.register(&bystander, "onlooker").await;
    h.storage.put_session("older-token", &usr_id, now() + 60);
    h.storage.put_session("expired-token", &usr_id, now() - 1);
    h.storage
        .put_session("foreign-token", &other_id, now() + 60);
    let token = second_session(&h, &author, "lister").await;

    let response = list_sessions(&h, &token).await;
    response.assert_status(StatusCode::OK);
    let body = response.text();
    for secret in [token.as_str(), "older-token", "foreign-token"] {
        assert!(!body.contains(secret), "the list leaks {secret:?}");
    }
    let list: SessionList = response.json();
    let current: Vec<bool> = list.sessions.iter().map(|s| s.current).collect();
    assert_eq!(current, [true, false], "{list:?}");
    assert!(list.sessions[0].created_at > list.sessions[1].created_at);
    assert!(list.sessions.iter().all(|s| s.expires_at > now()));
    assert!(list.sessions.iter().all(|s| s.id.len() == SESSION_ID_LEN));
    assert_ne!(list.sessions[0].id, list.sessions[1].id);
    h.client.assert_conformance();
}
