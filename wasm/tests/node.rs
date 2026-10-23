//! End-to-end checks of the JS-facing API, run under Node:
//! `cargo test -p crisp-wasm --target wasm32-unknown-unknown`.
#![cfg(target_arch = "wasm32")]

use crisp_wasm::format::{
    Bundle, parse_amount_js, recipe_constraints, validate_draft_js, verify_profile_js,
};
use crisp_wasm::identity::{Signer, key_file_user_id};
use js_sys::{Array, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;

const FULL_RECIPE: &[u8] = include_bytes!("../../spec/compliance/valid/full-recipe.crisp");
const MISSING_MAGIC: &[u8] =
    include_bytes!("../../spec/compliance/invalid/E.MAGIC/missing-magic.crisp");
#[allow(clippy::cast_precision_loss)]
const MAX_STEPS: f64 = crisp_core::limits::MAX_STEPS as f64;
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
fn maps_cross_as_plain_objects() {
    let constraints = recipe_constraints().expect("constraints");
    let limits = get(&constraints, "limits");
    assert!(
        !limits.is_instance_of::<js_sys::Map>(),
        "a Rust map is not a JS Map"
    );
    assert_eq!(
        js_sys::Object::get_prototype_of(&limits),
        js_sys::Object::get_prototype_of(&js_sys::Object::new().into()),
        "a Rust map is a plain object"
    );
    assert_eq!(get(&limits, "maxSteps").as_f64(), Some(MAX_STEPS));
}

#[wasm_bindgen_test]
fn keysets_cross_as_uint8array() {
    let identity = Signer::generate().expect("identity");
    let signed = identity
        .sign_profile("alice", "Alice", 1_790_000_000.0)
        .expect("sign profile");
    let profile = verify_profile_js(&signed).expect("verify profile");
    let keyset = get(&profile, "keyset");
    let ed25519 = get(&keyset, "ed25519");
    let ml_dsa_65 = get(&keyset, "ml-dsa-65");
    assert!(
        ed25519.is_instance_of::<Uint8Array>(),
        "not an Array of numbers"
    );
    assert!(ml_dsa_65.is_instance_of::<Uint8Array>());
    assert_eq!(Uint8Array::from(ed25519).length(), 32);
    assert_eq!(Uint8Array::from(ml_dsa_65).length(), 1952);
}

#[wasm_bindgen_test]
fn core_failures_carry_their_stable_code() {
    let Err(error) = Bundle::open(MISSING_MAGIC) else {
        panic!("a file without the magic opens");
    };
    assert!(error.is_instance_of::<js_sys::Error>());
    assert_eq!(
        get(&error, "name").as_string().as_deref(),
        Some("CrispFailure")
    );
    assert_eq!(get(&error, "code").as_string().as_deref(), Some("E.MAGIC"));
    assert!(
        get(&error, "status").is_undefined(),
        "only server failures carry a status"
    );
}

#[wasm_bindgen_test]
fn non_integer_timestamps_are_refused() {
    let bundle = Bundle::open(FULL_RECIPE).expect("open");
    let identity = Signer::generate().expect("identity");
    for seconds in [1.5, -1.0, f64::NAN, f64::INFINITY, 9.0e15] {
        let Err(error) = bundle.verify(Array::new().into(), seconds) else {
            panic!("verify accepted the timestamp {seconds}");
        };
        assert_eq!(
            get(&error, "message").as_string().as_deref(),
            Some("a timestamp must be whole epoch seconds"),
            "{seconds}"
        );
        assert_eq!(
            get(&error, "name").as_string().as_deref(),
            Some("CrispFailure")
        );
        assert!(get(&error, "code").is_undefined(), "{seconds}");
        assert!(
            identity.sign_profile("alice", "Alice", seconds).is_err(),
            "{seconds}"
        );
    }
    assert!(
        bundle.verify(Array::new().into(), 0.0).is_ok(),
        "epoch zero is whole"
    );
}

#[wasm_bindgen_test]
fn amounts_parse_and_constraints_load() {
    let parsed = parse_amount_js("1 1/2 cups", "en").expect("parse");
    assert_eq!(get(&parsed, "unit_known"), JsValue::TRUE);
    let constraints = recipe_constraints().expect("constraints");
    assert!(Array::from(&get(&constraints, "units")).length() > 20);
}
