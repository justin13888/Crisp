//! End-to-end checks of the JS-facing API, run under Node:
//! `cargo test -p crisp-wasm --target wasm32-unknown-unknown`.
#![cfg(target_arch = "wasm32")]

use crisp_wasm::amounts::{format_number_js, parse_amount_js, parse_temperature_js};
use crisp_wasm::format::{
    Bundle, core_info, image_blob, recipe_constraints, validate_draft_js, verify_profile_js,
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
    assert_eq!(
        get(&get(&constraints, "limits"), "maxSlugBytes").as_f64(),
        Some(64.0)
    );
}

#[wasm_bindgen_test]
fn absent_view_fields_are_omitted() {
    let bundle = Bundle::open(FULL_RECIPE).expect("open");
    let view = bundle.view(0, default_options()).expect("view");
    // The fixture has ingredients without an image and steps without a
    // temperature: those keys are absent, as the declarations say.
    let items = Array::from(&get(
        &Array::from(&get(&view, "ingredients")).get(0),
        "items",
    ));
    assert!(
        items
            .iter()
            .any(|item| !Reflect::has(&item, &"image".into()).expect("has"))
    );
    let steps = Array::from(&get(&view, "steps"));
    assert!(
        steps
            .iter()
            .any(|step| !Reflect::has(&step, &"temperature".into()).expect("has"))
    );
    // And no optional field anywhere crosses as `null`.
    let json = String::from(js_sys::JSON::stringify(&view).expect("stringify"));
    assert!(!json.contains(":null"), "a field crossed as null: {json}");
}

fn default_options() -> JsValue {
    js_sys::JSON::parse("{}").expect("json")
}

#[wasm_bindgen_test]
fn engine_helpers_format_parse_and_scale() {
    let half = js_sys::JSON::parse("[3,2]").expect("json");
    assert_eq!(
        format_number_js(half.clone(), "fraction".into()).expect("format"),
        "1½"
    );
    assert_eq!(
        format_number_js(half, "decimal".into()).expect("format"),
        "1.5"
    );
    assert!(format_number_js(JsValue::from(1.5), "decimal".into()).is_err());

    let temperature = parse_temperature_js("350-375 °F", "en").expect("temperature");
    assert_eq!(
        get(&temperature, "unit").as_string().as_deref(),
        Some("[degF]")
    );
    assert_eq!(
        js_sys::JSON::stringify(&get(&temperature, "max"))
            .expect("max")
            .as_string()
            .as_deref(),
        Some("[375,1]")
    );
    assert!(
        !Reflect::has(
            &parse_temperature_js("180", "en").expect("bare"),
            &"unit".into()
        )
        .expect("has")
    );
    assert!(parse_temperature_js("2 cups", "en").is_err());

    let bundle = Bundle::open(FULL_RECIPE).expect("open");
    let yield_amount = get(
        &bundle.view(0, default_options()).expect("view"),
        "yield_amount",
    );
    let factor = bundle.factor_for_yield(0, yield_amount).expect("factor");
    assert_eq!(
        js_sys::JSON::stringify(&factor)
            .expect("factor")
            .as_string()
            .as_deref(),
        Some("[1,1]")
    );
    assert!(
        bundle
            .factor_for_yield(9, js_sys::JSON::parse("[1,1]").expect("json"))
            .is_err()
    );

    let info = core_info().expect("info");
    assert_eq!(
        get(&info, "format_version_text").as_string().as_deref(),
        Some("1.0.0")
    );
}

const PIXEL: &[u8] = b"\x89PNG\r\n\x1a\n not a real image, only bytes";
const MAX_BLOB_SIZE: usize = crisp_core::limits::MAX_BLOB_SIZE;

#[wasm_bindgen_test]
fn image_blobs_admit_allowlisted_types_within_the_size_limit() {
    let image = image_blob(PIXEL, "image/png", 4, 0).expect("png");
    let blob = get(&image, "blob");
    assert_eq!(
        get(&blob, "media_type").as_string().as_deref(),
        Some("image/png")
    );
    assert_eq!(get(&blob, "width").as_f64(), Some(4.0));
    assert!(
        !Reflect::has(&blob, &"height".into()).expect("has"),
        "a height of 0 is omitted"
    );

    for (bytes, media_type, code) in [
        (PIXEL.to_vec(), "image/gif", "E.BLOB.MEDIATYPE"),
        (PIXEL.to_vec(), "Png", "E.BLOB.MEDIATYPE"),
        (vec![0; MAX_BLOB_SIZE + 1], "image/webp", "E.LIMIT"),
    ] {
        let Err(error) = image_blob(&bytes, media_type, 0, 0) else {
            panic!("{media_type} of {} bytes was admitted", bytes.len());
        };
        assert_eq!(get(&error, "code").as_string().as_deref(), Some(code));
    }
    assert!(image_blob(&vec![0; MAX_BLOB_SIZE], "image/webp", 0, 0).is_ok());
}

#[wasm_bindgen_test]
fn stored_blobs_cross_as_uint8array() {
    let image = image_blob(PIXEL, "image/png", 0, 0).expect("png");
    let id = get(&image, "id").as_string().expect("id");
    let draft = Bundle::open(FULL_RECIPE)
        .expect("open")
        .draft()
        .expect("draft");
    Reflect::set(
        &get(&draft, "blobs"),
        &id.as_str().into(),
        &get(&image, "blob"),
    )
    .expect("add blob");
    let identity = Signer::generate().expect("identity");
    let sealed = identity
        .seal(draft, "Tester", 1_790_000_000.0, None)
        .expect("seal");
    let bundle = Bundle::open(&sealed).expect("open");

    let blob = bundle.blob(&id).expect("blob");
    assert_eq!(
        get(&blob, "media_type").as_string().as_deref(),
        Some("image/png")
    );
    let bytes = get(&blob, "bytes");
    assert!(
        bytes.is_instance_of::<Uint8Array>(),
        "not an Array of numbers"
    );
    assert_eq!(Uint8Array::from(bytes).to_vec(), PIXEL);
    assert!(bundle.blob("unknown").expect("absent").is_undefined());
}
