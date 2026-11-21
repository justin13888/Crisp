//! The empty bundle a brand-new draft fills: `seal` derives every blank
//! field (ids, revision, identity, authors, times).

use std::collections::BTreeMap;

use crisp_core::FORMAT_VERSION;
use crisp_core::model::ids::{DocId, RecipeId, RevisionId};
use crisp_core::model::meta::DocumentMeta;
use crisp_core::model::payload::{BundlePayload, Kind};
use crisp_core::model::primitives::{LanguageTag, Quantity, Timestamp};
use crisp_core::model::recipe::{Identity, Recipe, Yield};
use crisp_core::model::text::LocalizedText;

/// A blank recipe for a brand-new draft; `seal` fills every derived field.
pub(super) fn blank_recipe() -> Recipe {
    Recipe {
        identity: Identity {
            author_fpr: Vec::new(),
            title: String::new(),
            language: LanguageTag(String::new()),
            created_at: Timestamp(0),
            birth_nonce: Vec::new(),
        },
        id: RecipeId(String::new()),
        revision: RevisionId(String::new()),
        supersedes: None,
        derived_from: None,
        modified_at: Timestamp(0),
        title: LocalizedText(BTreeMap::new()),
        summary: None,
        r#yield: Yield {
            amount: Quantity::default(),
            unit_text: LocalizedText(BTreeMap::new()),
        },
        times: None,
        ingredient_groups: None,
        ingredients: Vec::new(),
        equipment: None,
        step_groups: None,
        steps: Vec::new(),
        nutrition: None,
        tags: None,
        cuisine: None,
        course: None,
        diet: None,
        allergens: None,
        best_months: None,
        media: None,
        notes: None,
        source: None,
        attribution: None,
        ext: None,
    }
}

/// A blank single-recipe bundle.
pub(super) fn blank_payload(lang: &LanguageTag) -> BundlePayload {
    BundlePayload {
        v: FORMAT_VERSION,
        min_v: FORMAT_VERSION,
        kind: Kind::Recipe,
        doc_id: DocId(String::new()),
        default_language: lang.clone(),
        languages: vec![lang.clone()],
        meta: DocumentMeta {
            title: LocalizedText(BTreeMap::new()),
            authors: Vec::new(),
            license: "NONE".into(),
            created_at: Timestamp(0),
            modified_at: Timestamp(0),
            identity: None,
            description: None,
            cover: None,
            ext: None,
        },
        recipes: vec![blank_recipe()],
        book: None,
        blobs: BTreeMap::new(),
        ext: None,
    }
}
