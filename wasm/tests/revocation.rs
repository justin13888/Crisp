//! The JS-facing revocation API (spec §8.8), run under Node:
//! `cargo test -p crisp-wasm --target wasm32-unknown-unknown`.
#![cfg(target_arch = "wasm32")]

use crisp_core::codec::decode_file;
use crisp_core::crypto::hash::sha256;
use crisp_core::model::primitives::Timestamp;
use crisp_core::sign::SecretKeySet;
use crisp_core::verify::{PlatformKey, Revocation, sign_revocations};
use crisp_wasm::format::Bundle;
use crisp_wasm::revocation::{RevocationsView, accept_revocations};
use js_sys::{Array, Reflect};
use serde::Serialize;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

const REVOKED: &[u8] =
    include_bytes!("../../spec/compliance/invalid/E.SIG.REVOKED/revoked-endorsement.crisp");
const VERIFIED_AT: f64 = 1_800_000_000.0;

fn js<T: Serialize>(value: &T) -> JsValue {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    value.serialize(&serializer).expect("serialize")
}

fn get(value: &JsValue, key: &str) -> JsValue {
    Reflect::get(value, &key.into()).expect("property")
}

/// The compliance suite's platform key, which endorsed [`REVOKED`].
fn suite_key() -> PlatformKey {
    PlatformKey {
        keyset: SecretKeySet::from_seed(&[0xB1; 32]).public_keyset(),
        valid_from: Timestamp(1_700_000_000),
        valid_until: None,
    }
}

fn revocation_of(bytes: &[u8]) -> Revocation {
    let payload = decode_file(bytes).expect("decode").signed_bundle.payload;
    Revocation {
        content_hash: sha256(&payload).to_vec(),
        revoked_at: Timestamp(1_760_003_660),
    }
}

#[wasm_bindgen_test]
fn a_held_revocation_fails_the_endorsement() {
    let bundle = Bundle::open(REVOKED).expect("open");
    let keys = js(&vec![suite_key()]);
    let report = bundle
        .verify_with_revocations(keys.clone(), Array::new().into(), VERIFIED_AT)
        .expect("verify");
    assert_eq!(
        get(&report, "trust").as_string().as_deref(),
        Some("endorsed")
    );

    let revoked = js(&vec![revocation_of(REVOKED)]);
    let report = bundle
        .verify_with_revocations(keys, revoked, VERIFIED_AT)
        .expect("verify");
    assert_eq!(
        get(&report, "trust").as_string().as_deref(),
        Some("author-signed")
    );
    let e2 = Array::from(&get(&report, "checks"))
        .iter()
        .find(|c| get(c, "id").as_string().as_deref() == Some("E2"))
        .expect("E2");
    assert_eq!(
        get(&e2, "code").as_string().as_deref(),
        Some("E.SIG.REVOKED")
    );
}

#[wasm_bindgen_test]
fn accept_revocations_takes_only_a_list_the_pinned_key_signed() {
    let platform = SecretKeySet::from_seed(&[7; 32]);
    let pinned = js(&vec![PlatformKey {
        keyset: platform.public_keyset(),
        valid_from: Timestamp(0),
        valid_until: None,
    }]);
    let list = vec![revocation_of(REVOKED)];
    let signed = RevocationsView {
        signatures: vec![hex(&sign_revocations(&platform, &list).expect("sign"))],
        revocations: list,
    };
    let held = accept_revocations(pinned.clone(), Array::new().into(), js(&signed))
        .expect("a vouched list");
    assert_eq!(Array::from(&held).length(), 1);

    let unsigned = RevocationsView {
        signatures: vec!["not hex".to_owned()],
        revocations: signed.revocations,
    };
    assert!(accept_revocations(pinned, held, js(&unsigned)).is_err());
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
