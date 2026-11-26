//! The ingredient facet and index-name search text against PostgreSQL,
//! gated on `CRISP_TEST_DATABASE_URL` like `tests/postgres.rs`.

mod common;
mod pg;

use std::sync::Arc;

use common::{RecipeSpec, build_recipe_bundle_with, platform, test_config};
use crisp_server::search::RecipePage;
use crisp_server::state::App;
use crisp_server::storage::{PgStorage, Storage};
use kynos::http::StatusCode;
use kynos::test::TestClient;
use pg::{connect, fresh_identity, log_in};

/// Sorted titles of the search page at `path`.
async fn titles(client: &TestClient<App>, path: &str) -> Vec<String> {
    let response = client.get(path).send().await;
    response.assert_status(StatusCode::OK);
    let page: RecipePage = response.json();
    let mut t: Vec<String> = page.recipes.into_iter().map(|r| r.title).collect();
    t.sort();
    t
}

#[tokio::test]
async fn the_ingredient_facet_rolls_up_the_hierarchy_in_sql() {
    let Some(db) = connect("the_ingredient_facet_rolls_up_the_hierarchy_in_sql").await else {
        return;
    };
    let storage: Arc<dyn Storage> = Arc::new(PgStorage::new(db));
    let config = test_config();
    let service = crisp_server::router(&config.http)
        .build(App::new(storage, Arc::new(platform()), Arc::new(config)))
        .expect("router builds");
    let client = TestClient::new(service);
    let (author, username) = fresh_identity();
    let token = log_in(&client, &author, &username).await;

    // A run-unique cuisine keeps earlier runs' rows out of every page.
    let tag = format!("cuisine-{username}");
    for (title, code, substitute) in [
        ("Alpha", Some("kimchi-aged"), None),
        ("Beta", Some("kimchi"), None),
        ("Gamma", None, Some("kimchi")),
        ("Delta", None, None),
    ] {
        let spec = RecipeSpec {
            title: title.into(),
            cuisine: vec![tag.clone()],
            ingredient_code: code.map(str::to_owned),
            substitute_code: substitute.map(str::to_owned),
            ..RecipeSpec::default()
        };
        let (bytes, _, _) = build_recipe_bundle_with(&author, &spec);
        client
            .post("/api/v1/bundles")
            .header("authorization", &format!("Bearer {token}"))
            .body("application/octet-stream", bytes)
            .send()
            .await
            .assert_status(StatusCode::CREATED);
    }

    let search = |rest: &str| format!("/api/v1/search?cuisine={tag}&{rest}");
    // A parent finds its descendants; a leaf finds only itself.
    assert_eq!(
        titles(&client, &search("ingredient=kimchi")).await,
        vec!["Alpha", "Beta"]
    );
    assert_eq!(
        titles(&client, &search("ingredient=kimchi-aged")).await,
        vec!["Alpha"]
    );
    // Every listed ingredient must match.
    assert_eq!(
        titles(&client, &search("ingredient=kimchi,kimchi-aged")).await,
        vec!["Alpha"]
    );
    assert!(
        titles(&client, &search("ingredient=no-such-slug"))
            .await
            .is_empty()
    );
    // The index's names reach the full-text match: only the recipe coded
    // `kimchi-aged` carries the first word of its English name.
    let index = crisp_core::ingredients::index().expect("the embedded index loads");
    let name = index.display_name("kimchi-aged", "en").expect("named").name;
    let word = name.split_whitespace().next().expect("a word");
    assert!(word.is_ascii(), "{word:?} needs no URL encoding");
    assert_eq!(
        titles(&client, &search(&format!("q={word}"))).await,
        vec!["Alpha"]
    );
}
