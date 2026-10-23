//! End-to-end checks of the JS-facing API, run under Node:
//! `cargo test -p crisp-wasm --target wasm32-unknown-unknown`.
#![cfg(target_arch = "wasm32")]

use crisp_wasm::format::{Bundle, parse_amount_js, recipe_constraints, validate_draft_js};
use crisp_wasm::identity::{Signer, key_file_user_id};
use js_sys::{Array, Reflect};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

const FULL_RECIPE: &[u8] = include_bytes!("../../spec/compliance/valid/full-recipe.crisp");
const VERIFIED_AT: f64 = 1_800_000_000.0;

fn get(value: &JsValue, key: &str) -> JsValue {
    Reflect::get(value, &key.into()).expect("property")
}

#[wasm_bindgen_test]
fn opens_verifies_and_renders_a_published_recipe() {
    let bundle = Bundle::open(FULL_RECIPE).expect("open");
    let report = bundle
        .verify(Array::new().into(), VERIFIED_AT)
        .expect("verify");
    assert_eq!(get(&report, "compliant"), JsValue::TRUE);
    assert_eq!(
        get(&report, "trust").as_string().as_deref(),
        Some("author-signed")
    );

    let options = js_sys::JSON::parse(r#"{"scale":[2,1],"units":"metric"}"#).expect("json");
    let view = bundle.view(0, options).expect("view");
    assert_eq!(
        get(&view, "title").as_string().as_deref(),
        Some("Brown Butter Chocolate Chip Cookies")
    );
    assert_eq!(
        get(&view, "yield_text").as_string().as_deref(),
        Some("48 cookies")
    );
    let steps = Array::from(&get(&view, "steps"));
    assert_eq!(steps.length(), 5);
    let timers = Array::from(&get(&steps.get(0), "timers"));
    assert_eq!(get(&timers.get(0), "seconds").as_f64(), Some(300.0));
}

#[wasm_bindgen_test]
fn a_new_identity_seals_a_draft_that_verifies() {
    let template = Bundle::open(FULL_RECIPE).expect("open");
    let draft = template.draft().expect("draft");
    let issues = Array::from(&validate_draft_js(draft.clone()).expect("validate"));
    assert_eq!(issues.length(), 0);

    let identity = Signer::generate().expect("identity");
    let bytes = identity
        .seal(draft, "Tester", 1_790_000_000.0, None)
        .expect("seal");
    let sealed = Bundle::open(&bytes).expect("open");
    let report = sealed
        .verify(Array::new().into(), VERIFIED_AT)
        .expect("verify");
    assert_eq!(get(&report, "compliant"), JsValue::TRUE);
}

#[wasm_bindgen_test]
fn key_files_round_trip() {
    let identity = Signer::generate().expect("identity");
    let file = identity.wrap("a long passphrase").expect("wrap");
    assert_eq!(key_file_user_id(&file).expect("uid"), identity.user_id());
    let unlocked = Signer::unlock(&file, "a long passphrase").expect("unlock");
    assert_eq!(unlocked.user_id(), identity.user_id());
    assert!(Signer::unlock(&file, "the wrong passphrase").is_err());
}

#[wasm_bindgen_test]
fn amounts_parse_and_constraints_load() {
    let parsed = parse_amount_js("1 1/2 cups", "en").expect("parse");
    assert_eq!(get(&parsed, "unit_known"), JsValue::TRUE);
    let constraints = recipe_constraints().expect("constraints");
    assert!(Array::from(&get(&constraints, "units")).length() > 20);
}
