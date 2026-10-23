//! B3 over every bounded list the walk reports, one case per CDDL bound.

use std::collections::BTreeMap;

use super::tests::{at_and_over, paths_after};
use crate::fixtures::{amount, english, english_prose};
use crate::limits as l;
use crate::model::blob::Blob;
use crate::model::book::{BookStructure, Chapter, Entry, Page, Part};
use crate::model::ids::BlobId;
use crate::model::payload::BundlePayload;
use crate::model::primitives::OpenCode;
use crate::model::recipe::{
    AllergenCode, CourseCode, DietCode, Equipment, Nutrition, Source, StepIngredient, Substitute,
    Timer,
};

/// A book of one part holding one chapter with one entry.
fn one_entry_book() -> BookStructure {
    BookStructure {
        front_matter: None,
        parts: vec![Part {
            title: english("Part"),
            chapters: vec![Chapter {
                title: english("Chapter"),
                intro: None,
                entries: vec![Entry::Recipe { recipe: 0 }],
                ext: None,
            }],
            ext: None,
        }],
        back_matter: None,
    }
}

fn page() -> Page {
    Page {
        title: english("Page"),
        body: english_prose("Body."),
        blob: None,
        ext: None,
    }
}

fn blob(bytes: Vec<u8>) -> Blob {
    Blob {
        media_type: OpenCode::Other("x".to_owned()),
        size: 0,
        width: None,
        height: None,
        bytes,
    }
}

type Build = fn(&mut BundlePayload, usize);

/// Every bounded list, by its CDDL bound and the path it is reported at.
const LISTS: [(usize, &str, Build); 30] = [
    (l::MAX_RECIPES, "/recipes", |p, n| {
        p.recipes = vec![p.recipes[0].clone(); n];
    }),
    (l::MAX_LANGUAGES, "/languages", |p, n| {
        p.languages = vec![p.languages[0].clone(); n];
    }),
    (l::MAX_AUTHORS, "/meta/authors", |p, n| {
        p.meta.authors = vec![p.meta.authors[0].clone(); n];
    }),
    (l::MAX_BLOBS, "/blobs", |p, n| {
        p.blobs = (0..n)
            .map(|i| (BlobId(format!("b{i}")), blob(Vec::new())))
            .collect();
    }),
    (l::MAX_STEPS, "/recipes/0/steps", |p, n| {
        let r = &mut p.recipes[0];
        r.steps = vec![r.steps[0].clone(); n];
    }),
    (l::MAX_INGREDIENTS, "/recipes/0/ingredients", |p, n| {
        let r = &mut p.recipes[0];
        r.ingredients = vec![r.ingredients[0].clone(); n];
    }),
    (l::MAX_EQUIPMENT, "/recipes/0/equipment", |p, n| {
        let equipment = Equipment {
            r#ref: 0,
            name: english("Bowl"),
            quantity: None,
            note: None,
            blob: None,
            ext: None,
        };
        p.recipes[0].equipment = Some(vec![equipment; n]);
    }),
    (
        l::MAX_INGREDIENT_GROUPS,
        "/recipes/0/ingredient_groups",
        |p, n| p.recipes[0].ingredient_groups = Some(vec![english("Dough"); n]),
    ),
    (l::MAX_STEP_GROUPS, "/recipes/0/step_groups", |p, n| {
        p.recipes[0].step_groups = Some(vec![english("Bake"); n]);
    }),
    (l::MAX_TAGS, "/recipes/0/tags", |p, n| {
        p.recipes[0].tags = Some(vec!["sweet".to_owned(); n]);
    }),
    (l::MAX_CUISINE, "/recipes/0/cuisine", |p, n| {
        p.recipes[0].cuisine = Some(vec!["french".to_owned(); n]);
    }),
    (l::MAX_COURSE, "/recipes/0/course", |p, n| {
        p.recipes[0].course = Some(vec![OpenCode::Known(CourseCode::Main); n]);
    }),
    (l::MAX_DIET, "/recipes/0/diet", |p, n| {
        p.recipes[0].diet = Some(vec![OpenCode::Known(DietCode::Vegan); n]);
    }),
    (l::MAX_ALLERGENS, "/recipes/0/allergens", |p, n| {
        p.recipes[0].allergens = Some(vec![OpenCode::Known(AllergenCode::Milk); n]);
    }),
    (l::MAX_MEDIA, "/recipes/0/media", |p, n| {
        p.recipes[0].media = Some(vec![BlobId("b".to_owned()); n]);
    }),
    (l::MAX_ARRAY_LEN, "/recipes/0/nutrition/values", |p, n| {
        p.recipes[0].nutrition = Some(Nutrition {
            basis: amount(1, "g"),
            values: (0..n)
                .map(|i| (OpenCode::Other(format!("n{i}")), amount(1, "g")))
                .collect(),
        });
    }),
    (
        l::MAX_SUBSTITUTES,
        "/recipes/0/ingredients/0/substitutes",
        |p, n| {
            let sub = Substitute {
                name: english("Oat flour"),
                quantity: None,
                ext: None,
            };
            p.recipes[0].ingredients[0].substitutes = Some(vec![sub; n]);
        },
    ),
    (
        l::MAX_ALT_QUANTITIES,
        "/recipes/0/ingredients/0/alt",
        |p, n| {
            p.recipes[0].ingredients[0].alt = Some(vec![amount(1, "g"); n]);
        },
    ),
    (
        l::MAX_STEP_INGREDIENTS,
        "/recipes/0/steps/0/ingredients",
        |p, n| {
            let used = StepIngredient {
                r#ref: 0,
                quantity: None,
            };
            p.recipes[0].steps[0].ingredients = Some(vec![used; n]);
        },
    ),
    (
        l::MAX_STEP_EQUIPMENT,
        "/recipes/0/steps/0/equipment",
        |p, n| p.recipes[0].steps[0].equipment = Some(vec![0; n]),
    ),
    (l::MAX_STEP_TIMERS, "/recipes/0/steps/0/timers", |p, n| {
        let timer = Timer {
            duration: amount(1, "min"),
            label: None,
            passive: None,
            ext: None,
        };
        p.recipes[0].steps[0].timers = Some(vec![timer; n]);
    }),
    (l::MAX_TEXT_BYTES, "/recipes/0/tags/0", |p, n| {
        p.recipes[0].tags = Some(vec!["a".repeat(n)]);
    }),
    (l::MAX_TEXT_BYTES, "/recipes/0/cuisine/0", |p, n| {
        p.recipes[0].cuisine = Some(vec!["a".repeat(n)]);
    }),
    (l::MAX_TEXT_BYTES, "/recipes/0/source/url", |p, n| {
        p.recipes[0].source = Some(Source {
            url: Some("a".repeat(n)),
            ..Source::default()
        });
    }),
    (l::MAX_BOOK_PARTS, "/book/parts", |p, n| {
        let mut book = one_entry_book();
        book.parts = vec![book.parts[0].clone(); n];
        p.book = Some(book);
    }),
    (
        l::MAX_CHAPTERS_PER_PART,
        "/book/parts/0/chapters",
        |p, n| {
            let mut book = one_entry_book();
            book.parts[0].chapters = vec![book.parts[0].chapters[0].clone(); n];
            p.book = Some(book);
        },
    ),
    (
        l::MAX_ENTRIES_PER_CHAPTER,
        "/book/parts/0/chapters/0/entries",
        |p, n| {
            let mut book = one_entry_book();
            book.parts[0].chapters[0].entries = vec![Entry::Recipe { recipe: 0 }; n];
            p.book = Some(book);
        },
    ),
    (l::MAX_MATTER_PAGES, "/book/front_matter", |p, n| {
        let mut book = one_entry_book();
        book.front_matter = Some(vec![page(); n]);
        p.book = Some(book);
    }),
    (l::MAX_MATTER_PAGES, "/book/back_matter", |p, n| {
        let mut book = one_entry_book();
        book.back_matter = Some(vec![page(); n]);
        p.book = Some(book);
    }),
    (l::MAX_BLOB_SIZE, "/blobs/b~1x/bytes", |p, n| {
        p.blobs = BTreeMap::from([(BlobId("b/x".to_owned()), blob(vec![0; n]))]);
    }),
];

#[test]
fn every_bounded_list_is_limited_at_its_bound() {
    for (max, path, build) in LISTS {
        at_and_over(max, path, build);
    }
}

#[test]
fn a_recipe_reports_its_limits_in_walk_order() {
    let paths = paths_after(|p| {
        p.recipes[0].identity.title = "a".repeat(l::MAX_TITLE_BYTES + 1);
        p.recipes[0].steps = vec![p.recipes[0].steps[0].clone(); l::MAX_STEPS + 1];
    });
    assert_eq!(paths, ["/recipes/0/identity/title", "/recipes/0/steps"]);
}
