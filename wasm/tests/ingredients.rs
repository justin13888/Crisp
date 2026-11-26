//! The JS-facing ingredient index (`docs/ingredient-index.md` §3.3), run
//! under Node: `cargo test -p crisp-wasm --target wasm32-unknown-unknown`.
#![cfg(target_arch = "wasm32")]

use crisp_wasm::ingredients::{ingredient_entry_js, ingredient_name_js, match_ingredient_js};
use js_sys::{Array, Reflect};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

fn get(value: &JsValue, key: &str) -> JsValue {
    Reflect::get(value, &key.into()).expect("property")
}

fn text(value: &JsValue, key: &str) -> Option<String> {
    get(value, key).as_string()
}

#[wasm_bindgen_test]
fn matches_cross_as_tagged_plain_objects() {
    let exact = match_ingredient_js("aubergine", "en-US", 5).expect("match");
    assert_eq!(text(&exact, "kind").as_deref(), Some("exact"));
    assert_eq!(text(&exact, "slug").as_deref(), Some("aubergine"));
    assert_eq!(text(&exact, "name").as_deref(), Some("eggplant"));

    let suggested = match_ingredient_js("kim", "en", 2).expect("match");
    assert_eq!(text(&suggested, "kind").as_deref(), Some("candidates"));
    let candidates = Array::from(&get(&suggested, "candidates"));
    assert_eq!(candidates.length(), 2);
    assert_eq!(text(&candidates.get(0), "slug").as_deref(), Some("kimchi"));

    let none = match_ingredient_js("zzzqqq", "en", 5).expect("match");
    assert_eq!(text(&none, "kind").as_deref(), Some("none"));
}

#[wasm_bindgen_test]
fn names_and_entries_cross_or_are_undefined() {
    let name = ingredient_name_js("butter", "fr").expect("name");
    assert_eq!(text(&name, "name").as_deref(), Some("beurre"));
    assert_eq!(text(&name, "shard").as_deref(), Some("fr-FR"));
    assert!(
        ingredient_name_js("not-an-entry", "en")
            .expect("name")
            .is_undefined()
    );

    let flour = ingredient_entry_js("flour").expect("entry");
    assert_eq!(text(&flour, "category").as_deref(), Some("baking"));
    assert_eq!(get(&flour, "has_density"), JsValue::TRUE);
    // A root entry has no parent key at all, rather than a `null` one.
    assert!(!Reflect::has(&flour, &"parent".into()).expect("has"));
    assert!(
        ingredient_entry_js("not-an-entry")
            .expect("entry")
            .is_undefined()
    );
}
