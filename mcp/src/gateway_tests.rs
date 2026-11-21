//! The gateway's pure parts and its offline behaviour.

use super::*;

#[tokio::test]
async fn without_a_server_every_call_explains_the_setup() {
    let gateway = NoServer;
    assert_eq!(gateway.url(), None);
    for error in [
        gateway.publish(vec![1]).await.expect_err("publish"),
        gateway.download("rcp1x").await.expect_err("download"),
        gateway
            .search(SearchQuery::default())
            .await
            .expect_err("search"),
    ] {
        assert!(error.0.contains("CRISP_SERVER_URL"), "{error}");
    }
}

#[test]
fn search_params_drop_blank_text_and_empty_filters() {
    let empty = search_params(SearchQuery {
        q: Some("  ".into()),
        ..SearchQuery::default()
    });
    assert_eq!(empty.q, None);
    assert_eq!(
        (empty.cuisine, empty.course, empty.diet, empty.allergen_free),
        (None, None, None, None)
    );
    assert_eq!(empty.limit, Some(20));
    let full = search_params(SearchQuery {
        q: Some("carbonara".into()),
        cuisine: vec!["italian".into(), "roman".into()],
        course: vec!["main".into()],
        diet: vec!["nut-free".into()],
        allergen_free: vec!["fish".into()],
        language: Some("en".into()),
        limit: Some(5),
    });
    assert_eq!(full.q.as_deref(), Some("carbonara"));
    assert_eq!(full.cuisine.as_deref(), Some("italian,roman"));
    assert_eq!(full.course.as_deref(), Some("main"));
    assert_eq!(full.diet.as_deref(), Some("nut-free"));
    assert_eq!(full.allergen_free.as_deref(), Some("fish"));
    assert_eq!(full.language.as_deref(), Some("en"));
    assert_eq!(full.limit, Some(5));
}

#[test]
fn download_statuses_distinguish_missing_from_taken_down() {
    assert_eq!(download_status("rcp1x", Some(404)), Some(Ok(None)));
    let gone = download_status("rcp1x", Some(410))
        .expect("handled")
        .expect_err("an error");
    assert!(
        gone.0.contains("taken down") && gone.0.contains("rcp1x"),
        "{gone}"
    );
    assert_eq!(download_status("rcp1x", Some(500)), None);
    assert_eq!(download_status("rcp1x", None), None);
}

#[test]
fn debug_output_names_the_server_but_no_secrets() {
    let config = ServerConfig {
        url: "https://crisp.example".into(),
        username: "me".into(),
    };
    let gateway = HttpGateway::new(config, Arc::new(Signer::new(None))).expect("gateway");
    let shown = format!("{gateway:?}");
    assert!(
        shown.starts_with("HttpGateway") && shown.contains("https://crisp.example"),
        "{shown}"
    );
}

#[test]
fn a_malformed_url_is_refused() {
    let config = ServerConfig {
        url: "not a url".into(),
        username: "me".into(),
    };
    let error = HttpGateway::new(config, Arc::new(Signer::new(None))).expect_err("refused");
    assert!(error.0.contains("CRISP_SERVER_URL"), "{error}");
}
