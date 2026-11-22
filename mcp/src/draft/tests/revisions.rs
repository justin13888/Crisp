//! What a revision keeps from its base: step handles, history links,
//! extension data, notices about unshown content, and only live photos.

use super::*;
use crisp_core::model::payload::BundlePayload;

#[test]
fn step_photos_follow_explicit_refs_not_positions() {
    let photo = BlobId("blb1photo".into());
    let mut base = to_payload(&sample(), None).expect("converts");
    // Refs as the web app assigns them: from a shared counter, not 1..n.
    for (step, handle) in base.recipes[0].steps.iter_mut().zip([7, 3, 9]) {
        step.r#ref = handle;
    }
    base.recipes[0].steps[1].blob = Some(photo.clone());

    let mut edited = from_payload(&base).expect("extracts").draft;
    let handles: Vec<Option<u64>> = edited.steps.iter().map(|s| s.handle).collect();
    assert_eq!(handles, [Some(7), Some(3), Some(9)]);
    let mut inserted = edited.steps[0].clone();
    inserted.handle = None;
    inserted.text = "Bring a large pot of water to the boil.".into();
    edited.steps.insert(0, inserted);
    edited.steps.remove(3);

    let revised = to_payload(&edited, Some(&base)).expect("converts");
    let steps = &revised.recipes[0].steps;
    let refs: Vec<u64> = steps.iter().map(|s| s.r#ref).collect();
    assert_eq!(refs, [1, 7, 3]);
    assert_eq!(steps[2].blob, Some(photo));
    assert!(steps[0].blob.is_none() && steps[1].blob.is_none());
}

#[test]
fn a_revision_keeps_the_history_links_and_extension_data() {
    use crisp_core::model::ids::{RecipeId, RevisionId};
    let mut base = to_payload(&sample(), None).expect("converts");
    let recipe = &mut base.recipes[0];
    recipe.revision = RevisionId("rev1current".into());
    recipe.supersedes = Some(RevisionId("rev1older".into()));
    recipe.derived_from = Some(RecipeId("rcp1parent".into()));
    recipe.ext = Some(BTreeMap::new());
    let revised = to_payload(&sample(), Some(&base)).expect("converts");
    let kept = &revised.recipes[0];
    assert_eq!(kept.revision, RevisionId("rev1current".into()));
    assert_eq!(kept.supersedes, Some(RevisionId("rev1older".into())));
    assert_eq!(kept.derived_from, Some(RecipeId("rcp1parent".into())));
    assert_eq!(kept.ext, base.recipes[0].ext);
}

#[test]
fn each_kind_of_unshown_content_is_noticed_on_its_own() {
    let kept_notice = |payload: &crisp_core::model::payload::BundlePayload| {
        from_payload(payload)
            .expect("extracts")
            .notices
            .iter()
            .any(|n| n.starts_with("Not shown in the draft"))
    };
    let plain = to_payload(&sample(), None).expect("converts");
    assert!(!kept_notice(&plain));
    let cases: [fn(&mut BundlePayload); 6] = [
        |p| {
            p.recipes[0].ingredients[0].recipe =
                Some(crisp_core::model::ids::RecipeId("rcp1x".into()))
        },
        |p| p.recipes[0].ingredients[0].blob = Some(BlobId("blb1photo".into())),
        |p| p.recipes[0].steps[0].blob = Some(BlobId("blb1photo".into())),
        |p| {
            p.recipes[0].equipment.as_mut().expect("equipment")[0].blob =
                Some(BlobId("blb1photo".into()))
        },
        |p| p.ext = Some(BTreeMap::new()),
        |p| p.meta.ext = Some(BTreeMap::new()),
    ];
    for (i, change) in cases.iter().enumerate() {
        let mut payload = plain.clone();
        change(&mut payload);
        assert!(kept_notice(&payload), "case {i}");
    }
    let mut substitute = plain.clone();
    substitute.recipes[0].ingredients[1]
        .substitutes
        .as_mut()
        .expect("substitutes")[0]
        .ext = Some(BTreeMap::new());
    let notices = from_payload(&substitute).expect("extracts").notices;
    assert!(
        notices.iter().any(|n| n.contains("substitutes or timers")),
        "{notices:?}"
    );
}

#[test]
fn photos_nothing_references_are_dropped_from_a_revision() {
    use crisp_core::model::blob::{Blob, MediaType};
    use crisp_core::model::primitives::OpenCode;
    let blob = |byte: u8| Blob {
        media_type: OpenCode::Known(MediaType::Png),
        size: 1,
        width: None,
        height: None,
        bytes: vec![byte],
    };
    let (kept, dropped) = (BlobId("blb1kept".into()), BlobId("blb1dropped".into()));
    let mut base = to_payload(&sample(), None).expect("converts");
    base.recipes[0].steps[0].blob = Some(kept.clone());
    base.recipes[0].steps[2].blob = Some(dropped.clone());
    base.blobs.insert(kept.clone(), blob(1));
    base.blobs.insert(dropped.clone(), blob(2));

    let mut edited = from_payload(&base).expect("extracts").draft;
    edited.steps.remove(2);
    let revised = to_payload(&edited, Some(&base)).expect("converts");
    assert!(revised.blobs.contains_key(&kept));
    assert!(!revised.blobs.contains_key(&dropped));
}

#[test]
fn a_revision_keeps_ingredient_index_codes() {
    let mut base = to_payload(&sample(), None).expect("converts");
    let ingredient = &mut base.recipes[0].ingredients[1];
    ingredient.code = Some("pork-guanciale".into());
    ingredient.substitutes.as_mut().expect("has a substitute")[0].code =
        Some("pork-pancetta".into());

    let mut edited = from_payload(&base).expect("extracts").draft;
    edited.ingredients[0].name = "rigatoni".into();
    let revised = to_payload(&edited, Some(&base)).expect("converts");
    let kept = &revised.recipes[0].ingredients[1];
    assert_eq!(kept.code.as_deref(), Some("pork-guanciale"));
    let substitute = &kept.substitutes.as_ref().expect("has a substitute")[0];
    assert_eq!(substitute.code.as_deref(), Some("pork-pancetta"));

    edited.ingredients[1].substitutes[0].name = "bacon".into();
    let renamed = to_payload(&edited, Some(&base)).expect("converts");
    let substitute = &renamed.recipes[0].ingredients[1]
        .substitutes
        .as_ref()
        .expect("has a substitute")[0];
    assert!(
        substitute.code.is_none(),
        "a renamed substitute drops the old code"
    );
}

#[test]
fn renumbered_ingredients_do_not_inherit_another_items_photo() {
    let photo = BlobId("blb1spaghetti".into());
    let mut base = to_payload(&sample(), None).expect("converts");
    base.recipes[0].ingredients[0].blob = Some(photo.clone());
    base.recipes[0].ingredients[1].code = Some("pork-guanciale".into());

    // Drop spaghetti and renumber the rest from 1, as a model might.
    let mut edited = from_payload(&base).expect("extracts").draft;
    edited.ingredients.remove(0);
    for (ingredient, handle) in edited.ingredients.iter_mut().zip(1..) {
        ingredient.handle = handle;
    }
    let revised = to_payload(&edited, Some(&base)).expect("converts");
    let ingredients = &revised.recipes[0].ingredients;
    assert!(ingredients.iter().all(|i| i.blob.is_none()));
    assert!(ingredients.iter().all(|i| i.code.is_none()));

    // Kept refs keep their data, even when other items move around them.
    let mut edited = from_payload(&base).expect("extracts").draft;
    edited.ingredients.swap(0, 1);
    let revised = to_payload(&edited, Some(&base)).expect("converts");
    let by_ref = |r: u64| {
        revised.recipes[0]
            .ingredients
            .iter()
            .find(|i| i.r#ref == r)
            .expect("kept")
    };
    assert_eq!(by_ref(1).blob, Some(photo));
    assert_eq!(by_ref(2).code.as_deref(), Some("pork-guanciale"));
}

#[test]
fn a_new_step_takes_the_lowest_ref_no_step_uses() {
    let base = to_payload(&sample(), None).expect("converts");
    let mut edited = from_payload(&base).expect("extracts").draft;
    let handles: Vec<Option<u64>> = edited.steps.iter().map(|s| s.handle).collect();
    assert_eq!(handles, [Some(1), Some(2), Some(3)]);
    let mut added = edited.steps[0].clone();
    added.handle = None;
    edited.steps.push(added);
    let revised = to_payload(&edited, Some(&base)).expect("converts");
    let refs: Vec<u64> = revised.recipes[0].steps.iter().map(|s| s.r#ref).collect();
    assert_eq!(refs, [1, 2, 3, 4]);
}

#[test]
fn renumbered_equipment_does_not_inherit_another_items_photo() {
    let photo = BlobId("blb1skillet".into());
    let mut base = to_payload(&sample(), None).expect("converts");
    base.recipes[0].equipment.as_mut().expect("has equipment")[0].blob = Some(photo.clone());

    let mut edited = from_payload(&base).expect("extracts").draft;
    let kept = to_payload(&edited, Some(&base)).expect("converts");
    assert_eq!(
        kept.recipes[0].equipment.as_ref().expect("kept")[0].blob,
        Some(photo)
    );

    edited.equipment[0].handle = 2;
    for step in &mut edited.steps {
        for handle in &mut step.equipment {
            *handle = 2;
        }
    }
    let moved = to_payload(&edited, Some(&base)).expect("converts");
    assert!(
        moved.recipes[0].equipment.as_ref().expect("kept")[0]
            .blob
            .is_none()
    );
}
