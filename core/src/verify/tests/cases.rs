//! The published compliance cases: one valid or singly-invalid bundle
//! per entry, written to `spec/compliance/` by the suite.

use super::*;

pub(super) fn cases() -> Vec<Case> {
    use ErrorCode as E;
    vec![
        Case {
            name: "minimal",
            expect: None,
            build: || file(&published(&prepared(&draft_payload()))),
        },
        Case {
            name: "full-recipe",
            expect: None,
            build: || file(&published(&prepared(&rich_draft()))),
        },
        Case {
            name: "missing-magic",
            expect: Some(("A1", E::Magic)),
            build: || {
                let envelope = published(&prepared(&draft_payload()));
                to_canonical_cbor(&envelope).expect("encode")
            },
        },
        Case {
            name: "container-version",
            expect: Some(("A2", E::Container)),
            build: || {
                let mut envelope = published(&prepared(&draft_payload()));
                envelope.container_version = 2;
                file(&envelope)
            },
        },
        Case {
            name: "truncated",
            expect: Some(("A3", E::CborMalformed)),
            build: || {
                let mut bytes = file(&published(&prepared(&draft_payload())));
                bytes.truncate(bytes.len() - 10);
                bytes
            },
        },
        Case {
            name: "non-shortest-integer",
            expect: Some(("A4", E::CborNonDeterministic)),
            build: || {
                let bytes = file(&published(&prepared(&draft_payload())));
                // Tags (3 + 5 bytes), the 2-array head, then container version 1.
                assert_eq!(&bytes[8..10], &[0x82, 0x01]);
                [&bytes[..9], &[0x18, 0x01], &bytes[10..]].concat()
            },
        },
        Case {
            name: "indefinite-array",
            expect: Some(("A4", E::CborIndefinite)),
            build: || {
                let bytes = file(&published(&prepared(&draft_payload())));
                [&bytes[..8], &[0x9f], &bytes[9..], &[0xff]].concat()
            },
        },
        Case {
            name: "duplicate-key",
            expect: Some(("A5", E::CborDupKey)),
            build: || {
                let mut envelope = published(&prepared(&draft_payload()));
                envelope.signed_bundle.protected = vec![0xa2, 0x01, 0x01, 0x01, 0x02];
                file(&envelope)
            },
        },
        Case {
            name: "embedded-tag",
            expect: Some(("A5", E::CborTag)),
            build: || {
                let mut envelope = published(&prepared(&draft_payload()));
                let mut tagged = vec![0xd9, 0xd9, 0xf7];
                tagged.extend_from_slice(&envelope.signed_bundle.protected);
                envelope.signed_bundle.protected = tagged;
                file(&envelope)
            },
        },
        Case {
            name: "unknown-field",
            expect: Some(("B1", E::Schema)),
            build: || {
                raw_payload_edit(|entries| {
                    entries.push((Value::Text("bogus".to_owned()), Value::Bool(true)));
                })
            },
        },
        Case {
            name: "null-field",
            expect: Some(("B1", E::Schema)),
            build: || {
                raw_payload_edit(|entries| {
                    entries.push((Value::Text("ext".to_owned()), Value::Null));
                })
            },
        },
        Case {
            name: "future-reader",
            expect: Some(("B2", E::Version)),
            build: || mutated(|p| p.min_v = FORMAT_VERSION + 0x0100),
        },
        Case {
            name: "best-months-out-of-range",
            expect: Some(("B1", E::Schema)),
            build: || {
                mutated(|p| {
                    p.recipes[0].best_months = Some(vec![12, 13]);
                })
            },
        },
        Case {
            name: "best-months-unordered",
            expect: Some(("B1", E::Schema)),
            build: || {
                mutated(|p| {
                    p.recipes[0].best_months = Some(vec![7, 6]);
                })
            },
        },
        Case {
            name: "unknown-ingredient-code",
            expect: None,
            build: || {
                mutated(|p| {
                    p.recipes[0].ingredients[0].code = Some("not-in-any-index".to_owned());
                })
            },
        },
        Case {
            name: "ingredient-code-malformed",
            expect: Some(("B1", E::Schema)),
            build: || {
                mutated(|p| {
                    p.recipes[0].ingredients[0].code = Some("Aged Kimchi".to_owned());
                })
            },
        },
        Case {
            name: "substitute-code-malformed",
            expect: Some(("B1", E::Schema)),
            build: || {
                mutated(|p| {
                    p.recipes[0].ingredients[0].substitutes = Some(vec![Substitute {
                        name: crate::fixtures::english("rye flour"),
                        code: Some("rye--flour".to_owned()),
                        quantity: None,
                        ext: None,
                    }]);
                })
            },
        },
        Case {
            name: "too-many-timers",
            expect: Some(("B3", E::Limit)),
            build: || {
                mutated(|p| {
                    let timer = crate::model::recipe::Timer {
                        duration: crate::fixtures::amount(1, "min"),
                        label: None,
                        passive: None,
                        ext: None,
                    };
                    p.recipes[0].steps[0].timers = Some(vec![timer; 9]);
                })
            },
        },
        Case {
            name: "undeclared-default-language",
            expect: Some(("B4", E::Consistency)),
            build: || mutated(|p| p.default_language = LanguageTag("fr".to_owned())),
        },
        Case {
            name: "blob-hash",
            expect: Some(("C1", E::BlobHash)),
            build: || {
                mutated(|p| {
                    let id = crate::identity::derive_blob_id(b"other").expect("id");
                    p.blobs.insert(id.clone(), png_blob(b"image"));
                    p.recipes[0].media = Some(vec![id]);
                })
            },
        },
        Case {
            name: "blob-size",
            expect: Some(("C1", E::BlobSize)),
            build: || {
                mutated(|p| {
                    let id = crate::identity::derive_blob_id(b"image").expect("id");
                    let mut blob = png_blob(b"image");
                    blob.size = 99;
                    p.blobs.insert(id.clone(), blob);
                    p.recipes[0].media = Some(vec![id]);
                })
            },
        },
        Case {
            name: "blob-media-type",
            expect: Some(("C1", E::BlobMediaType)),
            build: || {
                mutated(|p| {
                    let id = crate::identity::derive_blob_id(b"image").expect("id");
                    let mut blob = png_blob(b"image");
                    blob.media_type = OpenCode::Other("image/svg+xml".to_owned());
                    p.blobs.insert(id.clone(), blob);
                    p.recipes[0].media = Some(vec![id]);
                })
            },
        },
        Case {
            name: "id-checksum",
            expect: Some(("C2", E::IdChecksum)),
            build: || {
                mutated(|p| {
                    let mut forged = p.recipes[0].id.0.clone();
                    forged.replace_range(10..11, if &forged[10..11] == "q" { "p" } else { "q" });
                    p.recipes[0].derived_from = Some(RecipeId(forged));
                })
            },
        },
        Case {
            name: "id-mismatch",
            expect: Some(("C3", E::IdMismatch)),
            build: || {
                mutated(|p| {
                    let mut other = p.recipes[0].identity.clone();
                    other.birth_nonce = vec![1; 16];
                    p.recipes[0].id = crate::identity::derive_recipe_id(&other).expect("id");
                })
            },
        },
        Case {
            name: "rev-mismatch",
            expect: Some(("C3", E::RevMismatch)),
            build: || {
                let mut payload = prepared(&draft_payload());
                let other = crate::identity::derive_revision(&prepared(&rich_draft()).recipes[0])
                    .expect("rev");
                payload.recipes[0].revision = other;
                file(&published(&payload))
            },
        },
        Case {
            name: "content-hash",
            expect: Some(("C4", E::ContentHash)),
            build: || {
                let payload = prepared(&draft_payload());
                let mut envelope = published(&payload);
                let mut bytes = envelope.signed_bundle.payload.clone();
                // Same-length, still-canonical edit: the license "CC0-1.0" → "CC0-1.1".
                let at = bytes
                    .windows(7)
                    .position(|w| w == b"CC0-1.0")
                    .expect("license");
                bytes[at + 6] = b'1';
                envelope.signed_bundle.payload = bytes;
                file(&envelope)
            },
        },
        Case {
            name: "author-binding",
            expect: Some(("C5", E::AuthorBinding)),
            build: || mutated(|p| p.meta.authors[0].uid = platform().user_id().expect("uid")),
        },
        Case {
            name: "unresolved-ref",
            expect: Some(("C6", E::Ref)),
            build: || {
                mutated(|p| {
                    p.recipes[0].steps[0].ingredients = Some(vec![StepIngredient {
                        r#ref: 99,
                        quantity: None,
                    }]);
                })
            },
        },
        Case {
            name: "http-link",
            expect: Some(("D1", E::UrlScheme)),
            build: || {
                mutated(|p| {
                    p.recipes[0].source = Some(crate::model::recipe::Source {
                        url: Some("http://example.com".to_owned()),
                        ..Default::default()
                    });
                })
            },
        },
        Case {
            name: "missing-default-language",
            expect: Some(("D2", E::L10nDefault)),
            build: || {
                mutated(|p| {
                    let fr = LanguageTag("fr".to_owned());
                    p.languages.push(fr.clone());
                    p.recipes[0].ingredients[0].name = crate::model::text::LocalizedText(
                        BTreeMap::from([(fr, "Farine".to_owned())]),
                    );
                })
            },
        },
        Case {
            name: "undeclared-language",
            expect: Some(("D2", E::L10nUndeclared)),
            build: || {
                mutated(|p| {
                    p.recipes[0].ingredients[0]
                        .name
                        .0
                        .insert(LanguageTag("de".to_owned()), "Mehl".to_owned());
                })
            },
        },
        Case {
            name: "time-order",
            expect: Some(("D3", E::TimeOrder)),
            build: || mutated(|p| p.meta.created_at = Timestamp(AUTHORED_AT.0 + 1)),
        },
        Case {
            name: "empty-quantity",
            expect: Some(("D4", E::Quantity)),
            build: || mutated(|p| p.recipes[0].ingredients[0].quantity = Some(Quantity::default())),
        },
        Case {
            name: "owner-unsigned",
            expect: Some(("E1", E::SigMissing)),
            build: || {
                // A listed co-author signs; the recipe's own author never does.
                let co_author = SecretKeySet::from_seed(&[0xD1; 32]);
                let mut payload = prepared(&draft_payload());
                payload.meta.authors.push(crate::model::meta::Author {
                    uid: co_author.user_id().expect("uid"),
                    display_name: "Co-author".to_owned(),
                    keyset: co_author.public_keyset(),
                });
                let signed = author_sign(&payload, &co_author, AUTHORED_AT, Signing::Deterministic)
                    .expect("sign");
                file(
                    &endorse_with(&signed, &platform(), ENDORSED_AT, Signing::Deterministic)
                        .expect("endorse"),
                )
            },
        },
        Case {
            name: "bad-author-signature",
            expect: Some(("E1", E::SigAuthor)),
            build: || {
                let payload = prepared(&draft_payload());
                let mut signed =
                    author_sign(&payload, &author(), AUTHORED_AT, Signing::Deterministic)
                        .expect("sign");
                signed.signed_bundle.signatures[0].sig_value[100] ^= 1;
                file(
                    &endorse_with(&signed, &platform(), ENDORSED_AT, Signing::Deterministic)
                        .expect("endorse"),
                )
            },
        },
        Case {
            name: "unknown-platform-key",
            expect: Some(("E2", E::KeyUnknown)),
            build: || {
                let payload = prepared(&draft_payload());
                let signed = author_sign(&payload, &author(), AUTHORED_AT, Signing::Deterministic)
                    .expect("sign");
                let stranger = SecretKeySet::from_seed(&[0xC1; 32]);
                file(
                    &endorse_with(&signed, &stranger, ENDORSED_AT, Signing::Deterministic)
                        .expect("endorse"),
                )
            },
        },
        Case {
            name: "retired-platform-key",
            expect: Some(("E2", E::KeyWindow)),
            build: || {
                let payload = prepared(&draft_payload());
                let signed = author_sign(&payload, &author(), AUTHORED_AT, Signing::Deterministic)
                    .expect("sign");
                file(
                    &endorse_with(
                        &signed,
                        &retired_platform(),
                        ENDORSED_AT,
                        Signing::Deterministic,
                    )
                    .expect("endorse"),
                )
            },
        },
        Case {
            name: "bad-endorsement",
            expect: Some(("E2", E::SigEndorsement)),
            build: || {
                let mut envelope = published(&prepared(&draft_payload()));
                envelope.signed_bundle.signatures[1].sig_value[100] ^= 1;
                file(&envelope)
            },
        },
        Case {
            name: "unbound-endorsement",
            expect: Some(("E2", E::SigBinding)),
            build: || {
                let mut envelope = published(&prepared(&draft_payload()));
                resign(&mut envelope, 1, &platform(), |sp| {
                    sp.binds = Some(vec![0; 32])
                });
                file(&envelope)
            },
        },
        Case {
            name: "modified-after-endorsement",
            expect: Some(("E3", E::TimeOrder)),
            build: || {
                let payload = prepared(&draft_payload());
                let signed = author_sign(&payload, &author(), AUTHORED_AT, Signing::Deterministic)
                    .expect("sign");
                let early = Timestamp(AUTHORED_AT.0 - ENDORSEMENT_SKEW_SECS - 1);
                file(
                    &endorse_with(&signed, &platform(), early, Signing::Deterministic)
                        .expect("endorse"),
                )
            },
        },
        trust::case(),
    ]
}
