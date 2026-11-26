//! Search and browse request handling, over `MockStorage`.
//!
//! The mock records the `SearchQuery` each request parsed into; matching
//! itself is the SQL's, asserted in `tests/postgres.rs`.

mod common;

use common::{Harness, RecipeSpec, build_recipe_bundle_with};
use crisp_core::sign::SecretKeySet;
use crisp_server::search::{DEFAULT_LIMIT, MAX_INGREDIENTS, MAX_LIMIT, RecipePage};
use crisp_server::storage::SearchQuery;
use crisp_server::version::ServerInfo;
use kynos::http::StatusCode;

#[tokio::test]
async fn search_facets_are_comma_separated() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[40u8; 32]);
    let token = h.register_and_login(&author, "olga").await;
    for (title, cuisine) in [
        ("Tacos", vec!["mexican", "street"]),
        ("Pho", vec!["vietnamese", "street"]),
        ("Pie", vec!["american"]),
    ] {
        let spec = RecipeSpec {
            title: title.into(),
            cuisine: cuisine.into_iter().map(str::to_owned).collect(),
            ..RecipeSpec::default()
        };
        let (bytes, _, _) = build_recipe_bundle_with(&author, &spec);
        h.publish(&token, bytes)
            .await
            .assert_status(StatusCode::CREATED);
    }

    let searched = |path: &'static str| {
        let client = &h.client;
        let storage = &h.storage;
        async move {
            client.get(path).send().await.assert_status(StatusCode::OK);
            storage
                .inner
                .lock()
                .unwrap()
                .searches
                .pop()
                .expect("search reached storage")
        }
    };
    let strings = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        searched("/api/v1/search?cuisine=street").await,
        SearchQuery {
            cuisine: strings(&["street"]),
            limit: DEFAULT_LIMIT,
            ..SearchQuery::default()
        }
    );
    assert_eq!(
        searched("/api/v1/search?cuisine=street,mexican").await,
        SearchQuery {
            cuisine: strings(&["street", "mexican"]),
            limit: DEFAULT_LIMIT,
            ..SearchQuery::default()
        }
    );
    // Every facet is trimmed and blank entries dropped; paging passes through.
    assert_eq!(
        searched(
            "/api/v1/search?q=%20taco%20&cuisine=%20street%20,,mexican&course=main\
             &diet=vegan,&allergen_free=gluten,milk&language=en&limit=200&offset=3\
             &ingredient=%20no-such-slug,"
        )
        .await,
        SearchQuery {
            text: Some("taco".into()),
            cuisine: strings(&["street", "mexican"]),
            course: strings(&["main"]),
            diet: strings(&["vegan"]),
            allergen_free: strings(&["gluten", "milk"]),
            ingredients: vec![strings(&["no-such-slug"])],
            language: Some("en".into()),
            limit: MAX_LIMIT,
            offset: 3,
        }
    );
    // Blank free text is absent rather than an empty match.
    assert_eq!(
        searched("/api/v1/search?q=%20%20&language=").await,
        SearchQuery {
            limit: DEFAULT_LIMIT,
            ..SearchQuery::default()
        }
    );

    let page: RecipePage = h
        .client
        .get("/api/v1/recipes?limit=2&offset=1")
        .send()
        .await
        .json();
    assert_eq!((page.limit, page.offset, page.recipes.len()), (2, 1, 2));

    let response = h.client.get("/api/v1/recipes?limit=0").send().await;
    response.assert_status(StatusCode::BAD_REQUEST);
    h.client.assert_conformance();
}

#[tokio::test]
async fn the_ingredient_facet_rolls_up_the_embedded_hierarchy() {
    let h = Harness::new();
    h.client
        .get("/api/v1/search?ingredient=kimchi,kimchi-aged")
        .send()
        .await
        .assert_status(StatusCode::OK);
    let query = h.storage.inner.lock().unwrap().searches.pop().unwrap();
    let [kimchi, aged] = query.ingredients.as_slice() else {
        panic!("one entry per requested slug: {query:?}");
    };
    assert!(kimchi.iter().any(|c| c == "kimchi"));
    assert!(kimchi.iter().any(|c| c == "kimchi-aged"), "{kimchi:?}");
    assert!(aged.iter().any(|c| c == "kimchi-aged"));
    assert!(!aged.iter().any(|c| c == "kimchi"), "{aged:?}");

    // The slug count is capped before any expansion work.
    let nine = ["kimchi"; MAX_INGREDIENTS + 1].join(",");
    h.client
        .get(&format!("/api/v1/search?ingredient={nine}"))
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST);
    assert!(h.storage.inner.lock().unwrap().searches.is_empty());
    h.client.assert_conformance();
}

#[tokio::test]
async fn coded_ingredients_are_indexed_with_their_index_names() {
    let h = Harness::new();
    let author = SecretKeySet::from_seed(&[41u8; 32]);
    let token = h.register_and_login(&author, "minji").await;
    let spec = RecipeSpec {
        title: "Kimchi jjigae".into(),
        ingredient_code: Some("kimchi-aged".into()),
        substitute_code: Some("kimchi".into()),
        ..RecipeSpec::default()
    };
    let (bytes, _, _) = build_recipe_bundle_with(&author, &spec);
    h.publish(&token, bytes)
        .await
        .assert_status(StatusCode::CREATED);

    let row = h.storage.inner.lock().unwrap().recipes.pop().unwrap();
    // Only the ingredient's own code is faceted, not its substitute's.
    assert_eq!(row.ingredient_codes, serde_json::json!(["kimchi-aged"]));
    let index = crisp_core::ingredients::index().unwrap();
    let english = index.display_name("kimchi-aged", "en").unwrap().name;
    assert!(
        row.search_text.contains(english),
        "{english:?} in {:?}",
        row.search_text
    );
    // The author's own ingredient name stays in the text too.
    assert!(row.search_text.contains("Flour"));
}

#[tokio::test]
async fn server_info_reports_the_embedded_ingredient_index() {
    let h = Harness::new();
    let info: ServerInfo = h.client.get("/api/v1/server-info").send().await.json();
    let embedded = crisp_core::ingredients::index().unwrap().version();
    assert_eq!(info.ingredient_index_version.as_deref(), Some(embedded));
    assert_eq!(
        embedded,
        include_str!("../../spec/ingredients/VERSION").trim()
    );
    h.client.assert_conformance();
}
