//! Unit tests for the parent module.

use super::*;
use crate::fixtures::sample_payload;
use crate::model::recipe::{AllergenCode, Nutrition};

#[test]
fn a_pointer_token_escapes_tilde_before_slash() {
    assert_eq!(pointer("/ext", "a.b"), "/ext/a.b");
    assert_eq!(pointer("/ext", "a/b~c"), "/ext/a~1b~0c");
    assert_eq!(pointer("", "~1"), "/~01");
}

#[test]
fn walk_value_visits_every_node_in_pre_order() {
    let value = Value::Map(vec![
        (
            Value::Text("a/b".to_owned()),
            Value::Array(vec![Value::Null]),
        ),
        (
            Value::Integer(7.into()),
            Value::Tag(0, Box::new(Value::Bool(true))),
        ),
        (Value::Bool(false), Value::Null),
    ]);
    let mut seen = Vec::new();
    walk_value("", &value, &mut |path, v| {
        seen.push((path.to_owned(), format!("{v:?}")));
    });
    let paths: Vec<&str> = seen.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        paths,
        [
            "", "/a~1b", "/a~1b", "/a~1b/0", "/7", "/7", "/7", "/?", "/?"
        ]
    );
    assert!(seen[6].1.contains("Bool(true)"), "{seen:?}");
}

/// Records every slug and count the walk reports.
#[derive(Default)]
struct Recorder(Vec<String>);

impl<'a> Visitor<'a> for Recorder {
    fn slug(&mut self, path: &str, slug: &'a str, kind: SlugKind) {
        self.0.push(format!("slug {path} {slug} {kind:?}"));
    }
    fn identity(&mut self, path: &str, _identity: &'a Identity) {
        self.0.push(format!("identity {path}"));
    }
}

#[test]
fn the_walk_reports_unknown_codes_and_tags_as_slugs() {
    let mut payload = sample_payload();
    let recipe = &mut payload.recipes[0];
    recipe.tags = Some(vec!["sweet".to_owned()]);
    recipe.allergens = Some(vec![
        OpenCode::Known(AllergenCode::Milk),
        OpenCode::Other("kiwi".to_owned()),
    ]);
    recipe.nutrition = Some(Nutrition {
        basis: crate::fixtures::amount(1, "g"),
        values: [(
            OpenCode::Other("x/y".to_owned()),
            crate::fixtures::amount(1, "g"),
        )]
        .into_iter()
        .collect(),
    });
    let mut recorder = Recorder::default();
    walk(&payload, &mut recorder);
    assert_eq!(
        recorder.0,
        [
            "identity /recipes/0/identity",
            "slug /recipes/0/nutrition/values/x~1y x/y Nutrient",
            "slug /recipes/0/tags/0 sweet Tag",
            "slug /recipes/0/allergens/1 kiwi Code",
        ]
    );
}
