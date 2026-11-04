//! Graceful platform key rotation over the real router: a server holding a
//! retired key publishes it after the current key with its `valid_until`,
//! signs the set with both, binds logins to the current key, and keeps
//! trusting what the retired key endorsed inside its window (spec §8.5).

mod common;

use common::{Harness, build_recipe_bundle, endorse_with, now, test_config};
use crisp_core::model::primitives::Timestamp;
use crisp_core::sign::SecretKeySet;
use crisp_core::verify::{PinUpdate, Trust, update_pin, verify};
use crisp_server::config::RetiredKey;
use crisp_server::platform::PlatformKey;
use crisp_server::version::ServerInfo;
use kynos::http::StatusCode;

/// The seed of the key the server retired.
const OLD_SEED: [u8; 32] = [41u8; 32];

/// A server that rotated from `OLD_SEED` to the test seed, the old key
/// retired at `valid_until`.
fn rotated(valid_until: u64) -> Harness {
    let mut config = test_config();
    config.platform_retired = vec![RetiredKey {
        seed: OLD_SEED,
        valid_until,
    }];
    Harness::with_config(config)
}

async fn server_info(h: &Harness) -> ServerInfo {
    let response = h.client.get("/api/v1/server-info").send().await;
    response.assert_status(StatusCode::OK);
    response.json()
}

#[tokio::test]
async fn a_rotated_server_publishes_both_keys_and_vouches_for_the_set() {
    let h = rotated(1_700_000_000);
    let info = server_info(&h).await;
    let current = common::platform();
    let old = PlatformKey::from_seed(&OLD_SEED).expect("old key");

    let ids: Vec<&str> = info
        .platform_keys
        .iter()
        .map(|k| k.usr_id.as_str())
        .collect();
    assert_eq!(ids, [current.user_id(), old.user_id()]);
    assert_eq!(info.platform_keys[0].valid_until, None);
    assert_eq!(info.platform_keys[1].valid_until, Some(1_700_000_000));
    let signers: Vec<&str> = info
        .key_set_signatures
        .iter()
        .map(|s| s.usr_id.as_str())
        .collect();
    assert_eq!(signers, ids);

    // A client that pinned the old key alone accepts the published set.
    let published = PlatformKey::new(
        &common::PLATFORM_SEED,
        &[RetiredKey {
            seed: OLD_SEED,
            valid_until: 1_700_000_000,
        }],
    )
    .expect("rotated key");
    let sigs: Vec<Vec<u8>> = info
        .key_set_signatures
        .iter()
        .map(|s| hex::decode(&s.sig).expect("hex signature"))
        .collect();
    let outcome = update_pin(&old.platform_keys(), &published.platform_keys(), &sigs);
    assert_eq!(
        outcome,
        PinUpdate::Updated {
            keys: published.platform_keys()
        }
    );

    // `platform-keys` lists the same set.
    let response = h.client.get("/api/v1/platform-keys").send().await;
    response.assert_status(StatusCode::OK);
    let keys: Vec<serde_json::Value> = response.json();
    assert_eq!(keys.len(), 2);

    // Logins stay bound to the current key.
    let author = SecretKeySet::from_seed(&[120u8; 32]);
    h.register(&author, "rotation").await;
    let (_, audience) = h.challenge("rotation").await;
    assert_eq!(audience, current.user_id());
    h.client.assert_conformance();
}

#[test]
fn endorsements_by_the_retired_key_verify_only_inside_its_window() {
    let (bytes, _) = build_recipe_bundle(&SecretKeySet::from_seed(&[121u8; 32]));
    let endorsed = endorse_with(OLD_SEED, &bytes);
    let at = Timestamp(u64::try_from(now()).expect("positive clock"));
    let trusted = |valid_until| {
        PlatformKey::new(
            &common::PLATFORM_SEED,
            &[RetiredKey {
                seed: OLD_SEED,
                valid_until,
            }],
        )
        .expect("rotated key")
        .platform_keys()
    };

    let report = verify(&endorsed, &trusted(at.0 + 3600), at);
    assert!(report.compliant, "{:?}", report.failures());
    assert_eq!(report.trust, Trust::Endorsed);

    let report = verify(&endorsed, &trusted(1_000), at);
    assert_eq!(report.trust, Trust::AuthorSigned);
    let codes: Vec<Option<&str>> = report
        .failures()
        .iter()
        .map(|c| c.code.as_deref())
        .collect();
    assert_eq!(codes, [Some("E.KEY.WINDOW")]);
}
