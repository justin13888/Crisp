//! Search and browse request handling, over `MockStorage`.
//!
//! The mock records the `SearchQuery` each request parsed into; matching
//! itself is the SQL's, asserted in `tests/postgres.rs`.

mod common;

use common::{Harness, RecipeSpec, build_recipe_bundle_with};
use crisp_core::sign::SecretKeySet;
use crisp_server::search::{DEFAULT_LIMIT, MAX_LIMIT, RecipePage};
use crisp_server::storage::SearchQuery;
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
             &diet=vegan,&allergen_free=gluten,milk&language=en&limit=200&offset=3"
        )
        .await,
        SearchQuery {
            text: Some("taco".into()),
            cuisine: strings(&["street", "mexican"]),
            course: strings(&["main"]),
            diet: strings(&["vegan"]),
            allergen_free: strings(&["gluten", "milk"]),
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
