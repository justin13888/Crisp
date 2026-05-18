//! Shared sample data for unit tests across the crate.
//!
//! Compiled only under `cfg(test)`. The recipe and payload built here are
//! structurally well-formed but carry placeholder identifiers and keys; tests
//! that need real ids derive them with [`crate::identity`].

use std::collections::BTreeMap;

use crate::model::ids::{DocId, RecipeId, RevisionId, UserId};
use crate::model::meta::{Author, DocumentMeta, PublicKeySet};
use crate::model::payload::{BundlePayload, Kind};
use crate::model::primitives::{LanguageTag, Quantity, Rational, Timestamp, UnitCode};
use crate::model::recipe::{Identity, Ingredient, Recipe, Step, Yield};
use crate::model::text::{LocalizedProse, LocalizedText, Paragraph, Prose, Run};

/// Builds an English-only [`LocalizedText`].
pub fn english(text: &str) -> LocalizedText {
    LocalizedText(BTreeMap::from([(
        LanguageTag("en".to_owned()),
        text.to_owned(),
    )]))
}

/// Builds an English-only single-paragraph [`LocalizedProse`].
pub fn english_prose(text: &str) -> LocalizedProse {
    let run = Run {
        t: text.to_owned(),
        style: None,
        link: None,
        reference: None,
    };
    LocalizedProse(BTreeMap::from([(
        LanguageTag("en".to_owned()),
        Prose(vec![Paragraph(vec![run])]),
    )]))
}

/// A frozen identity for the sample recipe.
pub fn sample_identity() -> Identity {
    Identity {
        author_fpr: vec![0u8; 32],
        title: "Cookies".to_owned(),
        language: LanguageTag("en".to_owned()),
        created_at: Timestamp(1_000),
        birth_nonce: vec![7u8; 16],
    }
}

/// A structurally well-formed recipe with placeholder ids.
pub fn sample_recipe() -> Recipe {
    Recipe {
        identity: sample_identity(),
        id: RecipeId("rcp-placeholder".to_owned()),
        revision: RevisionId("rev-placeholder".to_owned()),
        version: None,
        supersedes: None,
        derived_from: None,
        modified_at: Timestamp(2_000),
        title: english("Cookies"),
        summary: None,
        r#yield: Yield {
            amount: Quantity {
                min: Some(Rational::new(12, 1)),
                ..Quantity::default()
            },
            unit_text: english("cookies"),
        },
        times: None,
        ingredient_groups: None,
        ingredients: vec![Ingredient {
            r#ref: 0,
            group: None,
            name: english("Flour"),
            quantity: Some(Quantity {
                min: Some(Rational::new(2, 1)),
                unit: Some(UnitCode("cup".to_owned())),
                ..Quantity::default()
            }),
            display: None,
            preparation: None,
            optional: None,
            substitutes: None,
            blob: None,
        }],
        equipment: None,
        steps: vec![Step {
            r#ref: 0,
            group: None,
            text: english_prose("Mix and bake."),
            ingredient_refs: Some(vec![0]),
            equipment_refs: None,
            duration: None,
            temperature: None,
            produces: None,
            blob: None,
        }],
        nutrition: None,
        tags: None,
        cuisine: None,
        course: None,
        diet: None,
        allergens: None,
        media: None,
        notes: None,
        attribution: None,
        ext: None,
    }
}

/// A composite public key set with placeholder key bytes of the correct
/// lengths.
pub fn sample_keyset() -> PublicKeySet {
    PublicKeySet {
        ed25519: vec![1u8; 32],
        ml_dsa_65: vec![2u8; 1952],
    }
}

/// A single-recipe bundle payload built from [`sample_recipe`].
pub fn sample_payload() -> BundlePayload {
    BundlePayload {
        v: 0x01_0000,
        min_v: 0x01_0000,
        kind: Kind::Recipe,
        doc_id: DocId("rcp-placeholder".to_owned()),
        default_language: LanguageTag("en".to_owned()),
        languages: vec![LanguageTag("en".to_owned())],
        meta: DocumentMeta {
            title: english("Cookies"),
            authors: vec![Author {
                uid: UserId("usr-placeholder".to_owned()),
                display_name: "Baker".to_owned(),
                keyset: sample_keyset(),
            }],
            license: "CC0-1.0".to_owned(),
            created_at: Timestamp(1_000),
            modified_at: Timestamp(2_000),
            identity: None,
            description: None,
            cover: None,
        },
        recipes: vec![sample_recipe()],
        book: None,
        blobs: BTreeMap::new(),
        ext: None,
    }
}
