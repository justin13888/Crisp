//! Publishing, unreadable files, and the MCP surface itself.

use super::*;

#[tokio::test]
async fn publishing_stores_the_endorsed_file() {
    let (library, local_tools) = local();
    let saved = local_tools
        .save(&sample(), None, None)
        .await
        .expect("saves");
    assert!(
        local_tools
            .publish(&saved.doc_id)
            .await
            .expect_err("no server")
            .contains("CRISP_SERVER_URL")
    );

    let (_, other) = foreign_file();
    let endorsed = stored(&library, &saved.doc_id).await;
    let mut marked = endorsed.clone();
    marked.extend_from_slice(b"");
    let server = Arc::new(MockServer {
        publish_returns: Some(marked),
        ..MockServer::default()
    });
    let tools = authoring(Arc::clone(&library), server.clone());
    let published = tools.publish(&saved.doc_id).await.expect("publishes");
    assert_eq!(published.server, "https://crisp.test");
    assert_eq!(server.published.lock().expect("lock").len(), 1);

    let wrong = authoring(
        Arc::clone(&library),
        Arc::new(MockServer {
            publish_returns: Some(other),
            ..MockServer::default()
        }),
    );
    assert!(
        wrong
            .publish(&saved.doc_id)
            .await
            .expect_err("mismatch")
            .contains("different document")
    );
    let garbage = authoring(
        Arc::clone(&library),
        Arc::new(MockServer {
            publish_returns: Some(b"junk".to_vec()),
            ..MockServer::default()
        }),
    );
    assert!(
        garbage
            .publish(&saved.doc_id)
            .await
            .expect_err("junk")
            .contains("unreadable")
    );
    let unknown = "rcp1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";
    assert!(
        tools
            .publish(unknown)
            .await
            .expect_err("absent")
            .contains("save it first")
    );
}

#[tokio::test]
async fn publishing_refuses_a_returned_file_whose_author_signature_fails() {
    let (library, local_tools) = local();
    let saved = local_tools
        .save(&sample(), None, None)
        .await
        .expect("saves");
    let kept = stored(&library, &saved.doc_id).await;
    let mut broken = kept.clone();
    let last = broken.len() - 1;
    broken[last] ^= 0x01;
    let server = Arc::new(MockServer {
        publish_returns: Some(broken),
        ..MockServer::default()
    });
    let tools = authoring(Arc::clone(&library), server.clone());
    let refused = tools.publish(&saved.doc_id).await.expect_err("refused");
    assert!(
        refused.starts_with("the server returned a file that does not verify (phase E: E1"),
        "{refused}"
    );
    assert!(
        refused.contains("the library copy is unchanged"),
        "{refused}"
    );
    assert_eq!(server.published.lock().expect("lock").len(), 1);
    assert_eq!(library.read(&saved.doc_id).await.expect("read"), Some(kept));
}

#[tokio::test]
async fn publishing_never_overwrites_newer_or_unexpected_work() {
    let (library, tools) = local();
    let first = tools.save(&sample(), None, None).await.expect("saves");
    let first_bytes = stored(&library, &first.doc_id).await;
    let mut edited = sample();
    edited.title = "Carbonara, revised".into();
    tools
        .save(&edited, Some(&first.doc_id), None)
        .await
        .expect("revises");
    let second_bytes = stored(&library, &first.doc_id).await;

    // The server answers with an older revision of the same document.
    let stale = authoring(
        Arc::clone(&library),
        Arc::new(MockServer {
            publish_returns: Some(first_bytes.clone()),
            ..MockServer::default()
        }),
    );
    let refused = stale
        .publish(&first.doc_id)
        .await
        .expect_err("other revision");
    assert!(
        refused.contains("different document or revision"),
        "{refused}"
    );
    assert_eq!(
        library.read(&first.doc_id).await.expect("read"),
        Some(second_bytes.clone())
    );

    // A save lands while the upload is in flight: the library keeps it.
    library
        .write(&first.doc_id, &first_bytes)
        .await
        .expect("rewind");
    let racing = authoring(
        Arc::clone(&library),
        Arc::new(MockServer {
            saved_during_publish: Some((
                Arc::clone(&library),
                first.doc_id.clone(),
                second_bytes.clone(),
            )),
            ..MockServer::default()
        }),
    );
    let published = racing.publish(&first.doc_id).await.expect("publishes");
    assert!(!published.stored_endorsed);
    assert_eq!(published.revision, first.revision);
    assert_eq!(
        library.read(&first.doc_id).await.expect("read"),
        Some(second_bytes)
    );
}

#[tokio::test]
async fn an_edit_of_a_revision_that_has_moved_on_is_refused() {
    let (_, tools) = local();
    let first = tools.save(&sample(), None, None).await.expect("saves");
    let mut edited = sample();
    edited.title = "Carbonara, revised".into();
    let second = tools
        .save(&edited, Some(&first.doc_id), Some(&first.revision))
        .await
        .expect("the expected revision matches");
    let stale = tools
        .save(&sample(), Some(&first.doc_id), Some(&first.revision))
        .await
        .expect_err("revision moved on");
    assert!(
        stale.contains(&second.revision) && stale.contains("get_recipe"),
        "{stale}"
    );
    let orphan = tools
        .save(&sample(), None, Some(&second.revision))
        .await
        .expect_err("no base to compare with");
    assert!(orphan.contains("based_on"), "{orphan}");
}

#[tokio::test]
async fn a_file_that_is_not_the_requested_document_is_refused() {
    let (library, tools) = local();
    let saved = tools.save(&sample(), None, None).await.expect("saves");
    let (theirs, bytes) = foreign_file();
    library
        .write(&saved.doc_id, &bytes)
        .await
        .expect("misfiled");
    let error = tools.get(&saved.doc_id).await.expect_err("mismatch");
    assert!(
        error.contains(&theirs) && error.contains("different document"),
        "{error}"
    );
    let error = tools
        .save(&sample(), Some(&saved.doc_id), None)
        .await
        .expect_err("mismatch");
    assert!(error.contains("different document"), "{error}");
}

#[tokio::test]
async fn validation_against_a_base_matches_the_save() {
    let (_, tools) = local();
    let saved = tools.save(&sample(), None, None).await.expect("saves");
    let checked = tools
        .validate(&sample(), Some(&saved.doc_id))
        .await
        .expect("validates");
    assert!(checked.valid, "{checked:?}");
    let unknown = "rcp1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";
    assert!(tools.validate(&sample(), Some(unknown)).await.is_err());
}

#[tokio::test]
async fn unreadable_library_files_are_reported_not_fatal() {
    let (library, tools) = local();
    tools.save(&sample(), None, None).await.expect("saves");
    let junk = "rcp1zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
    library
        .write(junk, b"not a crisp file")
        .await
        .expect("write");
    let listed = tools.list(None).await.expect("lists");
    assert_eq!(listed.recipes.len(), 1);
    assert_eq!(listed.unreadable.len(), 1);
    assert!(listed.unreadable[0].starts_with(junk));
    assert!(
        tools
            .get(junk)
            .await
            .expect_err("unreadable")
            .contains("does not open")
    );
}

#[tokio::test]
async fn the_router_exposes_the_contracted_tools() {
    let hit = SearchHit {
        doc_id: "rcp1x".into(),
        title: "Carbonara".into(),
        language: "en".into(),
        authors: vec!["Other".into()],
        cuisine: vec!["italian".into()],
        course: vec![],
        diet: vec![],
        tags: vec![],
    };
    let library = Arc::new(MemoryLibrary::default());
    let server = Arc::new(MockServer {
        hits: vec![hit.clone()],
        ..MockServer::default()
    });
    let tools = CrispTools::new(authoring(library, server));
    let mut names: Vec<String> = tools
        .tool_router
        .list_all()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "crisp_status",
            "get_recipe",
            "list_recipes",
            "publish_recipe",
            "save_recipe",
            "search_server",
            "validate_recipe"
        ]
    );
    let info = tools.get_info();
    assert!(
        info.instructions
            .expect("instructions")
            .contains("validate_recipe")
    );

    assert_eq!(
        tools
            .search_server(Parameters(SearchQuery::default()))
            .await
            .expect("searches")
            .0
            .results,
        vec![hit]
    );
    assert!(tools.crisp_status().await.0.server.is_some());
    assert!(
        tools
            .validate_recipe(Parameters(ValidateInput {
                draft: sample(),
                based_on: None
            }))
            .await
            .expect("validates")
            .0
            .valid
    );
    let saved = tools
        .save_recipe(Parameters(SaveInput {
            draft: sample(),
            based_on: None,
            expected_revision: None,
        }))
        .await
        .expect("saves")
        .0;
    assert_eq!(
        tools
            .list_recipes(Parameters(ListInput::default()))
            .await
            .expect("lists")
            .0
            .recipes
            .len(),
        1
    );
    let doc = || {
        Parameters(DocInput {
            doc_id: saved.doc_id.clone(),
        })
    };
    assert_eq!(
        tools.get_recipe(doc()).await.expect("gets").0.doc_id,
        saved.doc_id
    );
    assert_eq!(
        tools
            .publish_recipe(doc())
            .await
            .expect("publishes")
            .0
            .doc_id,
        saved.doc_id
    );

    let offline = CrispTools::new(local().1);
    assert!(
        offline
            .search_server(Parameters(SearchQuery::default()))
            .await
            .is_err()
    );
}
