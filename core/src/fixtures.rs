//! Shared sample data for unit tests across the crate.
//!
//! Compiled only under `cfg(test)`. [`sample_payload`] is structurally
//! well-formed but carries placeholder identifiers and keys; [`draft_payload`]
//! is an editor draft ready for [`crate::author::seal`].

use std::collections::BTreeMap;

use crate::model::ids::{DocId, RecipeId, RevisionId, UserId};
use crate::model::meta::{Author, DocumentMeta, PublicKeySet};
use crate::model::payload::{BundlePayload, Kind};
use crate::model::primitives::{LanguageTag, Quantity, Rational, Timestamp, UnitCode};
use crate::model::recipe::{Identity, Ingredient, Recipe, Step, StepIngredient, Yield};
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

/// A quantity of `n` in `unit`.
pub fn amount(n: i64, unit: &str) -> Quantity {
    Quantity {
        min: Some(Rational::new(n, 1)),
        unit: Some(UnitCode(unit.to_owned())),
        ..Quantity::default()
    }
}

/// A structurally well-formed recipe with placeholder ids.
pub fn sample_recipe() -> Recipe {
    Recipe {
        identity: sample_identity(),
        id: RecipeId("rcp-placeholder".to_owned()),
        revision: RevisionId("rev-placeholder".to_owned()),
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
            quantity: Some(amount(2, "[cup_us]")),
            alt: None,
            fixed: None,
            preparation: None,
            note: None,
            optional: None,
            substitutes: None,
            recipe: None,
            blob: None,
            ext: None,
        }],
        equipment: None,
        step_groups: None,
        steps: vec![Step {
            r#ref: 0,
            group: None,
            text: english_prose("Mix and bake."),
            ingredients: Some(vec![StepIngredient {
                r#ref: 0,
                quantity: None,
            }]),
            equipment: None,
            timers: None,
            heat: None,
            temperature: None,
            produces: None,
            note: None,
            blob: None,
            ext: None,
        }],
        nutrition: None,
        tags: None,
        cuisine: None,
        course: None,
        diet: None,
        allergens: None,
        media: None,
        notes: None,
        source: None,
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
            ext: None,
        },
        recipes: vec![sample_recipe()],
        book: None,
        blobs: BTreeMap::new(),
        ext: None,
    }
}

/// An editor draft: [`sample_payload`] with every derived field blank and a
/// fixed birth nonce, as a UI would hand to [`crate::author::seal`].
pub fn draft_payload() -> BundlePayload {
    let mut payload = sample_payload();
    payload.doc_id = DocId(String::new());
    payload.meta.authors.clear();
    for recipe in &mut payload.recipes {
        recipe.id = RecipeId(String::new());
        recipe.revision = RevisionId(String::new());
        recipe.identity.author_fpr = Vec::new();
    }
    payload
}

fn ing(r#ref: u64, group: u64, name: &str, quantity: Option<Quantity>) -> Ingredient {
    Ingredient {
        r#ref,
        group: Some(group),
        name: english(name),
        quantity,
        alt: None,
        fixed: None,
        preparation: None,
        note: None,
        optional: None,
        substitutes: None,
        recipe: None,
        blob: None,
        ext: None,
    }
}

fn step(r#ref: u64, group: u64, text: &str, uses: &[(u64, Option<Quantity>)]) -> Step {
    Step {
        r#ref,
        group: Some(group),
        text: english_prose(text),
        ingredients: (!uses.is_empty()).then(|| {
            uses.iter()
                .map(|(r, q)| StepIngredient {
                    r#ref: *r,
                    quantity: q.clone(),
                })
                .collect()
        }),
        equipment: None,
        timers: None,
        heat: None,
        temperature: None,
        produces: None,
        note: None,
        blob: None,
        ext: None,
    }
}

fn frac(n: i64, d: u64, unit: &str) -> Quantity {
    Quantity {
        min: Some(Rational::new(n, d)),
        unit: Some(UnitCode(unit.to_owned())),
        ..Quantity::default()
    }
}

/// A complete, realistic recipe draft exercising every cooking feature:
/// groups, portions, timers (active and passive), heat, temperatures,
/// alternative measures, fixed and optional ingredients, substitutes,
/// equipment, notes, times, and source.
pub fn rich_draft() -> BundlePayload {
    use crate::model::primitives::OpenCode;
    use crate::model::recipe::{
        CourseCode, Equipment, HeatLevel, Source, Substitute, Temperature, TemperatureTarget,
        TemperatureUnit, Timer, Times,
    };

    let mut flour = ing(0, 0, "all-purpose flour", Some(frac(9, 4, "[cup_us]")));
    flour.alt = Some(vec![amount(280, "g")]);
    let mut butter = ing(1, 0, "unsalted butter", Some(amount(1, "{cup}")));
    butter.quantity = Some(frac(1, 1, "[cup_us]"));
    butter.preparation = Some(english("softened"));
    butter.substitutes = Some(vec![Substitute {
        name: english("salted butter (skip the salt)"),
        quantity: None,
        ext: None,
    }]);
    let mut salt = ing(4, 0, "flaky salt", Some(frac(1, 2, "[tsp_us]")));
    salt.fixed = Some(true);
    let mut chips = ing(6, 1, "dark chocolate", Some(amount(340, "g")));
    chips.preparation = Some(english("roughly chopped"));
    let mut nuts = ing(7, 1, "toasted walnuts", Some(frac(1, 1, "[cup_us]")));
    nuts.optional = Some(true);

    let mut cream = step(
        1,
        0,
        "Beat the butter and both sugars until pale and fluffy.",
        &[(1, None), (2, None), (3, None)],
    );
    cream.timers = Some(vec![Timer {
        duration: amount(3, "min"),
        label: Some(english("cream")),
        passive: None,
        ext: None,
    }]);
    let mut brown = step(
        0,
        0,
        "Brown half the butter over medium heat until it smells nutty, then cool.",
        &[(1, Some(frac(1, 2, "[cup_us]")))],
    );
    brown.heat = Some(HeatLevel::Medium);
    brown.timers = Some(vec![Timer {
        duration: Quantity {
            min: Some(Rational::new(5, 1)),
            max: Some(Rational::new(7, 1)),
            unit: Some(UnitCode("min".to_owned())),
            ..Quantity::default()
        },
        label: Some(english("brown butter")),
        passive: None,
        ext: None,
    }]);
    brown.note = Some(english_prose(
        "Watch closely: it goes from brown to burnt in seconds.",
    ));
    brown.equipment = Some(vec![1]);
    let mut chill = step(
        3,
        1,
        "Fold in the chocolate and walnuts, then chill the dough.",
        &[(6, None), (7, None)],
    );
    chill.timers = Some(vec![Timer {
        duration: amount(30, "min"),
        label: Some(english("chill")),
        passive: Some(true),
        ext: None,
    }]);
    chill.produces = Some(english("cookie dough"));
    let mut bake = step(
        4,
        2,
        "Scoop onto lined sheets, sprinkle with salt, and bake until the edges are golden.",
        &[(4, None)],
    );
    bake.temperature = Some(Temperature {
        min: Rational::new(180, 1),
        max: None,
        unit: TemperatureUnit::Celsius,
        target: Some(TemperatureTarget::Oven),
    });
    bake.timers = Some(vec![Timer {
        duration: Quantity {
            min: Some(Rational::new(10, 1)),
            max: Some(Rational::new(12, 1)),
            unit: Some(UnitCode("min".to_owned())),
            ..Quantity::default()
        },
        label: Some(english("bake")),
        passive: Some(true),
        ext: None,
    }]);
    bake.equipment = Some(vec![0]);

    let mut payload = draft_payload();
    let recipe = &mut payload.recipes[0];
    recipe.identity.title = "Brown Butter Chocolate Chip Cookies".to_owned();
    recipe.identity.birth_nonce = vec![0x42; 16];
    recipe.title = english("Brown Butter Chocolate Chip Cookies");
    recipe.summary = Some(english_prose("Nutty, chewy, and crisp at the edges."));
    recipe.r#yield = Yield {
        amount: Quantity {
            min: Some(Rational::new(24, 1)),
            ..Quantity::default()
        },
        unit_text: english("cookies"),
    };
    recipe.times = Some(Times {
        prep: Some(amount(20, "min")),
        cook: Some(amount(12, "min")),
        rest: Some(amount(30, "min")),
        total: Some(amount(65, "min")),
    });
    recipe.ingredient_groups = Some(vec![english("Dough"), english("Mix-ins")]);
    recipe.ingredients = vec![
        flour,
        butter,
        ing(2, 0, "light brown sugar", Some(frac(3, 4, "[cup_us]"))),
        ing(3, 0, "granulated sugar", Some(frac(1, 2, "[cup_us]"))),
        salt,
        ing(5, 0, "eggs", Some(amount(2, "{egg}"))),
        chips,
        nuts,
    ];
    recipe.equipment = Some(vec![
        Equipment {
            r#ref: 0,
            name: english("baking sheet"),
            quantity: Some(Quantity {
                min: Some(Rational::new(2, 1)),
                ..Quantity::default()
            }),
            note: Some(english("lined with parchment")),
            blob: None,
            ext: None,
        },
        Equipment {
            r#ref: 1,
            name: english("light-colored saucepan"),
            quantity: None,
            note: None,
            blob: None,
            ext: None,
        },
    ]);
    recipe.step_groups = Some(vec![english("Dough"), english("Rest"), english("Bake")]);
    recipe.steps = vec![
        brown,
        cream,
        step(
            2,
            0,
            "Beat in the eggs, then the flour, just until combined.",
            &[(5, None), (0, None)],
        ),
        chill,
        bake,
    ];
    recipe.course = Some(vec![
        OpenCode::Known(CourseCode::Dessert),
        OpenCode::Known(CourseCode::Baked),
    ]);
    recipe.cuisine = Some(vec!["american".to_owned()]);
    recipe.tags = Some(vec!["cookies".to_owned(), "make-ahead".to_owned()]);
    recipe.notes = Some(english_prose(
        "The dough keeps for three days in the fridge.",
    ));
    recipe.source = Some(Source {
        title: Some("Family recipe".to_owned()),
        author: None,
        url: Some("https://example.com/cookies".to_owned()),
    });
    payload.meta.title = english("Brown Butter Chocolate Chip Cookies");
    payload
}
