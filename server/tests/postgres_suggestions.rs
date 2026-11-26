//! Ingredient suggestions over real PostgreSQL, gated on
//! `CRISP_TEST_DATABASE_URL` like `tests/postgres.rs`: the per-account cap,
//! triage writing its audit row once, the export read's order and `since`
//! filter, the suggester becoming `NULL` on deletion, and the table's
//! `CHECK` constraints.
//!
//! Every test uses fresh identities, so they share one database with the
//! other PostgreSQL tests.

mod common;
mod pg;

use std::sync::Arc;

use common::{now, platform, test_config};
use crisp_server::db::{ingredient_suggestion, moderation_action};
use crisp_server::state::App;
use crisp_server::storage::{
    FileSuggestion, NewSuggestion, PgStorage, ResolveSuggestion, Storage, Suggestions,
};
use kynos::test::TestClient;
use pg::{connect, fresh_identity, log_in};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

/// A fresh, logged-in account's `usr…` id, registered through the router
/// over `db`.
async fn account(db: &sea_orm::DatabaseConnection) -> String {
    let storage: Arc<dyn Storage> = Arc::new(PgStorage::new(db.clone()));
    let config = test_config();
    let service = crisp_server::router(&config.http)
        .build(App::new(storage, Arc::new(platform()), Arc::new(config)))
        .expect("router builds");
    let client = TestClient::new(service);
    let (key, username) = fresh_identity();
    log_in(&client, &key, &username).await;
    key.user_id().unwrap().0
}

fn suggestion(suggester: &str, text: &str) -> NewSuggestion {
    NewSuggestion {
        suggester: suggester.to_owned(),
        kind: "name".to_owned(),
        slug: Some("kimchi-aged".to_owned()),
        locale: "en-US".to_owned(),
        text: text.to_owned(),
        note: String::new(),
        at: now(),
    }
}

/// Files `suggestion`, asserting it was stored.
async fn filed(storage: &PgStorage, suggestion: NewSuggestion) -> ingredient_suggestion::Model {
    match storage.file_suggestion(suggestion, 2).await.unwrap() {
        FileSuggestion::Filed(row) => *row,
        FileSuggestion::TooMany => panic!("the suggestion is filed"),
    }
}

#[tokio::test]
async fn suggestions_are_capped_triaged_once_and_outlive_their_suggester() {
    let Some(db) = connect("suggestions_are_capped_triaged_once").await else {
        return;
    };
    let usr_id = account(&db).await;
    let storage = PgStorage::new(db.clone());
    let first = filed(&storage, suggestion(&usr_id, "mugunji")).await;
    filed(&storage, suggestion(&usr_id, "old kimchi")).await;
    assert_eq!(
        storage
            .file_suggestion(suggestion(&usr_id, "third"), 2)
            .await
            .unwrap(),
        FileSuggestion::TooMany
    );

    let at = now();
    let ResolveSuggestion::Resolved {
        suggestion: triaged,
        audit,
    } = storage
        .resolve_suggestion(first.id, "accepted", "usr1admin", at)
        .await
        .unwrap()
    else {
        panic!("the suggestion is triaged");
    };
    assert_eq!(triaged.status, "accepted");
    assert_eq!(triaged.resolved_by.as_deref(), Some("usr1admin"));
    assert_eq!(audit.action, "resolve_suggestion");
    assert_eq!(audit.target, format!("suggestion/{}", first.id));
    let rows = moderation_action::Entity::find()
        .filter(moderation_action::Column::Target.eq(audit.target.clone()))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(rows, [audit]);
    let again = storage
        .resolve_suggestion(first.id, "declined", "usr1admin", at)
        .await;
    assert!(matches!(
        again.unwrap(),
        ResolveSuggestion::AlreadyResolved(_)
    ));
    let absent = storage.resolve_suggestion(-1, "declined", "usr1admin", at);
    assert_eq!(absent.await.unwrap(), ResolveSuggestion::Absent);
    // A triaged suggestion no longer counts against the cap.
    filed(&storage, suggestion(&usr_id, "third")).await;

    let accepted = storage.accepted_suggestions(at, 1000).await.unwrap();
    assert!(accepted.iter().any(|row| row.id == first.id));
    assert!(accepted.iter().all(|row| row.status == "accepted"));
    let later = storage.accepted_suggestions(at + 1, 1000).await.unwrap();
    assert!(later.iter().all(|row| row.id != first.id));
    let open = storage.suggestions(Some("open"), 200, 0).await.unwrap();
    assert!(open.iter().all(|row| row.status == "open"));

    storage.delete_account(&usr_id).await.unwrap();
    let kept = ingredient_suggestion::Entity::find_by_id(first.id)
        .one(&db)
        .await
        .unwrap();
    assert_eq!(kept.expect("the suggestion stays").suggester, None);
}

#[tokio::test]
async fn the_table_refuses_an_unknown_kind_or_status() {
    let Some(db) = connect("the_table_refuses_an_unknown_kind_or_status").await else {
        return;
    };
    for (kind, status) in [("rename", "open"), ("other", "closed")] {
        let insert = format!(
            "INSERT INTO ingredient_suggestions \
             (kind, locale, text, note, status, created_at) \
             VALUES ('{kind}', 'en-US', 'x', '', '{status}', 0)"
        );
        assert!(
            db.execute_unprepared(&insert).await.is_err(),
            "{kind}/{status} must be refused"
        );
    }
}
