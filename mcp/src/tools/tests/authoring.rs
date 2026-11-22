//! Status, validation, saving, listing, and reading.

use super::*;

#[tokio::test]
async fn status_reports_identity_library_server_and_vocabulary() {
    let (_, tools) = local();
    let status = tools.status().await;
    assert_eq!(status.user_id, Some(me().user_id().expect("uid").0));
    assert_eq!(status.signer_problem, None);
    assert_eq!(status.display_name.as_deref(), Some("Me"));
    assert_eq!(status.library, "memory");
    assert_eq!(status.server, None);
    assert!(status.vocabulary.course.contains(&"main".to_owned()));
    assert!(status.vocabulary.diet.contains(&"gluten-free".to_owned()));
    assert!(
        status
            .vocabulary
            .allergens
            .contains(&"tree-nuts".to_owned())
    );
    let en = LanguageTag("en".into());
    let unknown: Vec<&String> = status
        .vocabulary
        .units
        .iter()
        .filter(|unit| !parse_amount(&format!("1 {unit}"), &en).is_ok_and(|p| p.unit_known))
        .collect();
    assert!(unknown.is_empty(), "not catalogue units: {unknown:?}");

    let keyless = Authoring {
        signer: Arc::new(Signer::new(None)),
        ..tools
    };
    let status = keyless.status().await;
    assert_eq!(status.user_id, None);
    assert!(
        status
            .signer_problem
            .expect("a problem")
            .contains("CRISP_KEY_FILE")
    );
}

#[tokio::test]
async fn validation_reports_conversion_and_format_issues_on_the_draft() {
    let (_, tools) = local();
    assert_eq!(
        tools.validate(&sample(), None).await.expect("validates"),
        ValidateOutput {
            valid: true,
            issues: vec![]
        }
    );
    let mut bad = sample();
    bad.best_months = vec![13];
    bad.steps[0].heat = Some("volcanic".into());
    let conversion = tools.validate(&bad, None).await.expect("validates");
    assert!(!conversion.valid);
    assert_eq!(conversion.issues[0].path, "/steps/0/heat");
    bad.steps[0].heat = None;
    let format = tools.validate(&bad, None).await.expect("validates");
    assert!(!format.valid);
    let issue = format
        .issues
        .iter()
        .find(|i| i.path.starts_with("/best_months"))
        .unwrap_or_else(|| panic!("{format:?}"));
    assert!(issue.code.is_some());
}

#[tokio::test]
async fn a_new_recipe_is_saved_listed_read_and_revised() {
    let (library, tools) = local();
    let created = tools.save(&sample(), None, None).await.expect("saves");
    assert_eq!(created.outcome, SaveOutcome::Created);
    assert!(library.read(&created.doc_id).expect("read").is_some());

    let listed = tools.list(None).expect("lists");
    assert_eq!(listed.recipes.len(), 1);
    assert_eq!(listed.recipes[0].title, "Spaghetti alla carbonara");
    assert_eq!(listed.recipes[0].modified_at, NOW.0);
    assert_eq!(tools.list(Some("ROMAN")).expect("lists").recipes.len(), 1);
    assert_eq!(
        tools.list(Some("weeknight")).expect("lists").recipes.len(),
        1
    );
    assert_eq!(tools.list(Some("sushi")).expect("lists").recipes.len(), 0);
    assert_eq!(tools.list(Some("  ")).expect("lists").recipes.len(), 1);

    let got = tools.get(&created.doc_id).await.expect("gets");
    assert_eq!(got.draft, sample());
    assert!(got.verified);
    assert_eq!(got.found_in, "library");
    assert_eq!(got.authors, vec!["Me".to_owned()]);

    let mut edited = got.draft;
    edited.title = "Carbonara (revised)".into();
    let revised = tools
        .save(&edited, Some(&created.doc_id), None)
        .await
        .expect("revises");
    assert_eq!(revised.outcome, SaveOutcome::Revised);
    assert_eq!(revised.doc_id, created.doc_id);
    assert_ne!(revised.revision, created.revision);
    assert_eq!(
        tools.get(&created.doc_id).await.expect("gets").draft.title,
        "Carbonara (revised)"
    );
}

#[tokio::test]
async fn editing_someone_elses_recipe_forks_it() {
    let (library, tools) = local();
    let (theirs, bytes) = foreign_file();
    library.write(&theirs, &bytes).expect("write");
    let forked = tools
        .save(&sample(), Some(&theirs), None)
        .await
        .expect("forks");
    assert_eq!(forked.outcome, SaveOutcome::Forked);
    assert_ne!(forked.doc_id, theirs);
    let payload =
        open_draft(&library.read(&forked.doc_id).expect("read").expect("stored")).expect("opens");
    assert_eq!(
        payload.recipes[0]
            .derived_from
            .as_ref()
            .map(|r| r.0.as_str()),
        Some(theirs.as_str())
    );
}

#[tokio::test]
async fn a_server_recipe_can_be_read_and_forked_but_must_verify() {
    let (theirs, bytes) = foreign_file();
    let server = Arc::new(MockServer::default());
    server
        .files
        .lock()
        .expect("lock")
        .push((theirs.clone(), bytes.clone()));
    let library = Arc::new(MemoryLibrary::default());
    let tools = authoring(Arc::clone(&library), server.clone());
    let got = tools.get(&theirs).await.expect("gets");
    assert_eq!(got.found_in, "server");
    assert_eq!(got.authors, vec!["Other".to_owned()]);
    assert_eq!(
        tools
            .save(&got.draft, Some(&theirs), None)
            .await
            .expect("forks")
            .outcome,
        SaveOutcome::Forked
    );

    let mut tampered = bytes;
    let at = tampered
        .windows(9)
        .position(|w| w == b"Spaghetti")
        .expect("the title is in the file");
    tampered[at] = b'Z';
    server.files.lock().expect("lock")[0].1 = tampered;
    let refused = tools
        .save(&sample(), Some(&theirs), None)
        .await
        .expect_err("refused");
    assert!(refused.contains("does not verify"), "{refused}");
    let missing = tools
        .get(&theirs.replace('q', "p"))
        .await
        .expect_err("missing");
    assert!(missing.contains("or on the server"), "{missing}");
}

#[tokio::test]
async fn saving_explains_what_is_missing() {
    let (_, tools) = local();
    let mut bad = sample();
    bad.steps.clear();
    let error = tools.save(&bad, None, None).await.expect_err("invalid");
    assert!(error.contains("/steps"), "{error}");
    let mut bad_format = sample();
    bad_format.tags = vec!["Not A Slug".into()];
    assert!(
        tools
            .save(&bad_format, None, None)
            .await
            .expect_err("invalid")
            .contains("/tags/0")
    );

    let unknown = "rcp1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq";
    let error = tools
        .save(&sample(), Some(unknown), None)
        .await
        .expect_err("no base");
    assert!(
        error.contains("in the library") && !error.contains("server"),
        "{error}"
    );
    assert!(tools.save(&sample(), Some("../x"), None).await.is_err());

    let nameless = Authoring {
        display_name: None,
        ..tools.clone()
    };
    assert!(
        nameless
            .save(&sample(), None, None)
            .await
            .expect_err("no name")
            .contains("CRISP_DISPLAY_NAME")
    );
    let keyless = Authoring {
        signer: Arc::new(Signer::new(None)),
        ..tools
    };
    assert!(
        keyless
            .save(&sample(), None, None)
            .await
            .expect_err("no key")
            .contains("CRISP_KEY_FILE")
    );
}
