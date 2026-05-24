//! The staged verification pipeline (spec §11).
//!
//! [`verify`] runs the deterministic A–E procedure over a `.crisp` file and
//! emits a [`ComplianceReport`]. Phases A and B are fatal — the first failure
//! stops verification. Phases C, D, and E are collect-all — every failure is
//! reported. The bundle never has to be trusted: every field is treated as
//! attacker-controlled until a check clears it.

pub mod report;

pub use report::{CheckResult, CheckStatus, ComplianceClass, ComplianceReport, Trust};

use std::collections::BTreeSet;

use crate::FORMAT_VERSION;
use crate::cbor::scan_canonical;
use crate::codec::{decode_file, from_canonical_cbor, to_canonical_cbor};
use crate::crypto::bech32id::decode_id;
use crate::crypto::hash::sha256;
use crate::error::{CrispError, ErrorCode, Result};
use crate::identity::{derive_recipe_id, derive_revision, key_fingerprint};
use crate::limits;
use crate::model::book::Entry;
use crate::model::envelope::{CONTAINER_VERSION, CoseSign, ProtectedHeader, SigProtected, SigRole};
use crate::model::meta::PublicKeySet;
use crate::model::payload::{BundlePayload, Kind};
use crate::model::primitives::{LanguageTag, Quantity, Timestamp};
use crate::model::recipe::Recipe;
use crate::model::text::{LocalizedProse, LocalizedText};
use crate::sign::{signing_message, verify_composite};
use report::CRISP_REPORT_MARKER;

/// Clock-skew tolerance, in seconds, allowed when comparing a `modified_at`
/// against a server endorsement's `signed_at` (check E3).
const ENDORSEMENT_SKEW_SECS: u64 = 300;

/// Verifies a `.crisp` file and returns its compliance report.
///
/// `platform_keys` are the trusted server-endorsement public keys; pass an
/// empty slice for offline structural verification (check E2 is then skipped).
/// `verified_at` is stamped into the report.
#[must_use]
pub fn verify(
    bytes: &[u8],
    platform_keys: &[PublicKeySet],
    verified_at: Timestamp,
) -> ComplianceReport {
    let mut checks = Checks::new();

    let Some(decoded) = run_fatal_phases(&mut checks, bytes) else {
        return checks.finish(0, Trust::Unsigned, verified_at);
    };

    phase_c(&mut checks, &decoded);
    phase_d(&mut checks, &decoded.payload);
    let trust = phase_e(&mut checks, &decoded, platform_keys);

    checks.finish(decoded.payload.v, trust, verified_at)
}

/// The artifacts produced once the fatal phases (A, B) have passed.
struct Decoded {
    cose: CoseSign,
    protected: ProtectedHeader,
    payload: BundlePayload,
}

/// Runs phases A and B; returns `None` on the first fatal failure.
fn run_fatal_phases(checks: &mut Checks, bytes: &[u8]) -> Option<Decoded> {
    // A3/A4 — the whole file decodes as canonical CBOR.
    if let Err(e) = scan_canonical(bytes) {
        let id = if e.code() == Some(ErrorCode::CborMalformed) {
            "A3"
        } else {
            "A4"
        };
        checks.fail(id, &e);
        return None;
    }
    checks.pass("A3");
    checks.pass("A4");

    // A1 (framing tags) and the envelope schema part of B1.
    let envelope = match decode_file(bytes) {
        Ok(envelope) => envelope,
        Err(e) => {
            let id = if e.code() == Some(ErrorCode::Schema) {
                "B1"
            } else {
                "A1"
            };
            checks.fail(id, &e);
            return None;
        }
    };
    checks.pass("A1");

    // A2 — container version.
    if envelope.container_version != CONTAINER_VERSION {
        checks.fail(
            "A2",
            &CrispError::check(
                ErrorCode::Container,
                format!(
                    "unsupported container version {}",
                    envelope.container_version
                ),
            ),
        );
        return None;
    }
    checks.pass("A2");

    let cose = envelope.signed_bundle;

    // A5 — every embedded byte string is itself canonical CBOR.
    let mut embedded: Vec<&[u8]> = vec![&cose.protected, &cose.payload];
    for sig in &cose.signatures {
        embedded.push(&sig.protected);
    }
    for blob in embedded {
        if let Err(e) = scan_canonical(blob) {
            checks.fail("A5", &e);
            return None;
        }
    }
    checks.pass("A5");

    // B1 — the protected header and payload decode against the schema.
    let protected: ProtectedHeader = match from_canonical_cbor(&cose.protected) {
        Ok(value) => value,
        Err(e) => {
            checks.fail("B1", &e);
            return None;
        }
    };
    let payload: BundlePayload = match from_canonical_cbor(&cose.payload) {
        Ok(value) => value,
        Err(e) => {
            checks.fail("B1", &e);
            return None;
        }
    };
    checks.pass("B1");

    // B2 — version range.
    if let Err(e) = check_version(&payload) {
        checks.fail("B2", &e);
        return None;
    }
    checks.pass("B2");

    // B3 — §12 hard limits.
    if let Err(e) = check_limits(&payload) {
        checks.fail("B3", &e);
        return None;
    }
    checks.pass("B3");

    // B4 — internal consistency.
    if let Err(e) = check_consistency(&payload) {
        checks.fail("B4", &e);
        return None;
    }
    checks.pass("B4");

    Some(Decoded {
        cose,
        protected,
        payload,
    })
}

/// B2 — the format version is within this reader's supported range.
fn check_version(payload: &BundlePayload) -> Result<()> {
    if payload.min_v > FORMAT_VERSION {
        return Err(CrispError::check(
            ErrorCode::Version,
            format!(
                "bundle requires reader version {:#08x}, this reader is {FORMAT_VERSION:#08x}",
                payload.min_v
            ),
        ));
    }
    if payload.v >> 16 != FORMAT_VERSION >> 16 {
        return Err(CrispError::check(
            ErrorCode::Version,
            format!("unsupported format major version in {:#08x}", payload.v),
        ));
    }
    Ok(())
}

/// B3 — every §12 limit is respected.
fn check_limits(payload: &BundlePayload) -> Result<()> {
    limits::enforce(payload.recipes.len(), limits::MAX_RECIPES, "recipes")?;
    limits::enforce(payload.blobs.len(), limits::MAX_BLOBS, "blobs")?;
    limits::enforce(payload.languages.len(), limits::MAX_LANGUAGES, "languages")?;
    limits::enforce(payload.meta.authors.len(), limits::MAX_AUTHORS, "authors")?;

    let mut total_blob_bytes = 0usize;
    for blob in payload.blobs.values() {
        limits::enforce(blob.bytes.len(), limits::MAX_BLOB_SIZE, "blob size")?;
        total_blob_bytes = total_blob_bytes.saturating_add(blob.bytes.len());
    }
    limits::enforce(
        total_blob_bytes,
        limits::MAX_TOTAL_BLOB_BYTES,
        "total blob bytes",
    )?;

    for recipe in &payload.recipes {
        limits::enforce(recipe.steps.len(), limits::MAX_STEPS, "steps")?;
        limits::enforce(
            recipe.ingredients.len(),
            limits::MAX_INGREDIENTS,
            "ingredients",
        )?;
        if let Some(equipment) = &recipe.equipment {
            limits::enforce(equipment.len(), limits::MAX_EQUIPMENT, "equipment")?;
        }
        if let Some(groups) = &recipe.ingredient_groups {
            limits::enforce(
                groups.len(),
                limits::MAX_INGREDIENT_GROUPS,
                "ingredient groups",
            )?;
        }
        limits::enforce(
            recipe.identity.title.len(),
            limits::MAX_TITLE_BYTES,
            "identity title",
        )?;
        if let Some(tags) = &recipe.tags {
            limits::enforce(tags.len(), limits::MAX_TAGS, "tags")?;
        }
        if let Some(cuisine) = &recipe.cuisine {
            limits::enforce(cuisine.len(), limits::MAX_CUISINE, "cuisine")?;
        }
        if let Some(course) = &recipe.course {
            limits::enforce(course.len(), limits::MAX_COURSE, "course")?;
        }
        if let Some(diet) = &recipe.diet {
            limits::enforce(diet.len(), limits::MAX_DIET, "diet")?;
        }
        if let Some(allergens) = &recipe.allergens {
            limits::enforce(allergens.len(), limits::MAX_ALLERGENS, "allergens")?;
        }
        if let Some(media) = &recipe.media {
            limits::enforce(media.len(), limits::MAX_MEDIA, "media")?;
        }
        for ingredient in &recipe.ingredients {
            if let Some(substitutes) = &ingredient.substitutes {
                limits::enforce(
                    substitutes.len(),
                    limits::MAX_SUBSTITUTES,
                    "ingredient substitutes",
                )?;
            }
        }
        for step in &recipe.steps {
            if let Some(refs) = &step.ingredient_refs {
                limits::enforce(
                    refs.len(),
                    limits::MAX_STEP_INGREDIENT_REFS,
                    "step ingredient refs",
                )?;
            }
            if let Some(refs) = &step.equipment_refs {
                limits::enforce(
                    refs.len(),
                    limits::MAX_STEP_EQUIPMENT_REFS,
                    "step equipment refs",
                )?;
            }
        }
    }
    Ok(())
}

/// B4 — `kind`, `book`, `meta.identity`, and language declarations agree.
fn check_consistency(payload: &BundlePayload) -> Result<()> {
    let bad = |detail: &str| CrispError::check(ErrorCode::Consistency, detail.to_owned());

    if !payload.languages.contains(&payload.default_language) {
        return Err(bad("default_language is not listed in languages"));
    }
    match payload.kind {
        Kind::Recipe => {
            if payload.book.is_some() {
                return Err(bad("a recipe bundle must not carry a book structure"));
            }
            if payload.meta.identity.is_some() {
                return Err(bad("a recipe bundle must not carry meta.identity"));
            }
        }
        Kind::Book => {
            if payload.book.is_none() {
                return Err(bad("a book bundle must carry a book structure"));
            }
            if payload.meta.identity.is_none() {
                return Err(bad("a book bundle must carry meta.identity"));
            }
        }
    }
    Ok(())
}

/// Phase C — integrity & identity (collect-all).
fn phase_c(checks: &mut Checks, decoded: &Decoded) {
    let payload = &decoded.payload;

    // C1 — blobs are content-addressed and correctly sized.
    checks.run("C1", || {
        for (id, blob) in &payload.blobs {
            if blob.size as usize != blob.bytes.len() {
                return Err(CrispError::check(
                    ErrorCode::BlobSize,
                    format!(
                        "blob {} declares size {} but holds {} bytes",
                        id.0,
                        blob.size,
                        blob.bytes.len()
                    ),
                ));
            }
            let derived = crate::identity::derive_blob_id(&blob.bytes)?;
            if derived.0 != id.0 {
                return Err(CrispError::check(
                    ErrorCode::BlobHash,
                    format!("blob id {} does not match its content hash", id.0),
                ));
            }
        }
        Ok(())
    });

    // C2 — every identifier has a valid bech32m checksum and correct HRP.
    checks.run("C2", || {
        check_id(&payload.doc_id.0)?;
        for author in &payload.meta.authors {
            check_id(&author.uid.0)?;
        }
        for recipe in &payload.recipes {
            check_id(&recipe.id.0)?;
            check_id(&recipe.revision.0)?;
            if let Some(s) = &recipe.supersedes {
                check_id(&s.0)?;
            }
            if let Some(d) = &recipe.derived_from {
                check_id(&d.0)?;
            }
        }
        for id in payload.blobs.keys() {
            check_id(&id.0)?;
        }
        Ok(())
    });

    // C3 — each recipe's id and revision match what its content derives.
    checks.run("C3", || {
        for recipe in &payload.recipes {
            let derived_id = derive_recipe_id(&recipe.identity)?;
            if derived_id != recipe.id {
                return Err(CrispError::check(
                    ErrorCode::IdMismatch,
                    format!("recipe {} does not derive from its identity", recipe.id.0),
                ));
            }
            let derived_revision = derive_revision(recipe)?;
            if derived_revision != recipe.revision {
                return Err(CrispError::check(
                    ErrorCode::RevMismatch,
                    format!("recipe {} revision does not match its content", recipe.id.0),
                ));
            }
        }
        Ok(())
    });

    // C4 — the protected header's content hash equals SHA-256(payload).
    checks.run("C4", || {
        let actual = sha256(&decoded.cose.payload);
        if actual.as_slice() != decoded.protected.content_hash.as_slice() {
            return Err(CrispError::check(
                ErrorCode::ContentHash,
                "protected-header content hash does not match the payload",
            ));
        }
        Ok(())
    });

    // C5 — every recipe's author_fpr matches an embedded author key.
    checks.run("C5", || {
        let mut fingerprints = BTreeSet::new();
        for author in &payload.meta.authors {
            fingerprints.insert(key_fingerprint(&author.keyset)?.to_vec());
        }
        for recipe in &payload.recipes {
            if !fingerprints.contains(&recipe.identity.author_fpr) {
                return Err(CrispError::check(
                    ErrorCode::AuthorBinding,
                    format!("recipe {} author_fpr matches no author keyset", recipe.id.0),
                ));
            }
        }
        Ok(())
    });

    // C6 — every internal reference resolves.
    checks.run("C6", || check_references(payload));
}

/// Checks that an identifier string is valid bech32m.
fn check_id(id: &str) -> Result<()> {
    decode_id(id).map(|_| ())
}

/// C6 — resolves ingredient/equipment handles, group indices, blob
/// references, and book recipe entries.
fn check_references(payload: &BundlePayload) -> Result<()> {
    let bad = |detail: String| CrispError::check(ErrorCode::Ref, detail);

    for recipe in &payload.recipes {
        let ingredient_refs: BTreeSet<u64> = recipe.ingredients.iter().map(|i| i.r#ref).collect();
        let equipment_refs: BTreeSet<u64> =
            recipe.equipment.iter().flatten().map(|e| e.r#ref).collect();
        let group_count = recipe.ingredient_groups.as_ref().map_or(0, Vec::len) as u64;

        for ingredient in &recipe.ingredients {
            if let Some(group) = ingredient.group
                && group >= group_count
            {
                return Err(bad(format!(
                    "ingredient group index {group} is out of range"
                )));
            }
        }
        for step in &recipe.steps {
            for handle in step.ingredient_refs.iter().flatten() {
                if !ingredient_refs.contains(handle) {
                    return Err(bad(format!("step references unknown ingredient {handle}")));
                }
            }
            for handle in step.equipment_refs.iter().flatten() {
                if !equipment_refs.contains(handle) {
                    return Err(bad(format!("step references unknown equipment {handle}")));
                }
            }
        }
    }

    // Blob references resolve to keys in the blob store.
    for blob_ref in blob_references(payload) {
        if !payload.blobs.keys().any(|k| k.0 == blob_ref) {
            return Err(bad(format!(
                "blob reference {blob_ref} is not in the blob store"
            )));
        }
    }

    // Book recipe entries point at a real recipe.
    if let Some(book) = &payload.book {
        for part in &book.parts {
            for chapter in &part.chapters {
                for entry in &chapter.entries {
                    if let Entry::Recipe { recipe } = entry
                        && *recipe as usize >= payload.recipes.len()
                    {
                        return Err(bad(format!(
                            "book entry recipe index {recipe} is out of range"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Collects every blob id referenced (as opposed to stored) in the payload.
fn blob_references(payload: &BundlePayload) -> Vec<String> {
    let mut refs = Vec::new();
    if let Some(cover) = &payload.meta.cover {
        refs.push(cover.0.clone());
    }
    for recipe in &payload.recipes {
        for media in recipe.media.iter().flatten() {
            refs.push(media.0.clone());
        }
        for ingredient in &recipe.ingredients {
            if let Some(blob) = &ingredient.blob {
                refs.push(blob.0.clone());
            }
        }
        for step in &recipe.steps {
            if let Some(blob) = &step.blob {
                refs.push(blob.0.clone());
            }
        }
        for equipment in recipe.equipment.iter().flatten() {
            if let Some(blob) = &equipment.blob {
                refs.push(blob.0.clone());
            }
        }
    }
    refs
}

/// Phase D — policy (collect-all).
fn phase_d(checks: &mut Checks, payload: &BundlePayload) {
    let (texts, proses) = collect_localized(payload);
    let declared: BTreeSet<&LanguageTag> = payload.languages.iter().collect();

    // D1 — every run link uses the https scheme.
    checks.run("D1", || {
        for prose in &proses {
            for paragraph in prose.0.values().flat_map(|p| &p.0) {
                for run in &paragraph.0 {
                    if let Some(link) = &run.link
                        && !link.starts_with("https://")
                    {
                        return Err(CrispError::check(
                            ErrorCode::UrlScheme,
                            format!("run link {link:?} is not an https URL"),
                        ));
                    }
                }
            }
        }
        Ok(())
    });

    // D2 — localization is complete.
    checks.run("D2", || {
        if !declared.contains(&payload.default_language) {
            return Err(CrispError::check(
                ErrorCode::L10nUndeclared,
                "default_language is not declared in languages",
            ));
        }
        for recipe in &payload.recipes {
            if !declared.contains(&recipe.identity.language) {
                return Err(CrispError::check(
                    ErrorCode::L10nUndeclared,
                    "an identity language is not declared in languages",
                ));
            }
        }
        for text in &texts {
            check_localized_keys(text.0.keys(), &declared, &payload.default_language)?;
        }
        for prose in &proses {
            check_localized_keys(prose.0.keys(), &declared, &payload.default_language)?;
        }
        Ok(())
    });

    // D3 — created_at precedes modified_at.
    checks.run("D3", || {
        if payload.meta.created_at > payload.meta.modified_at {
            return Err(CrispError::check(
                ErrorCode::TimeOrder,
                "meta.created_at is after meta.modified_at",
            ));
        }
        for recipe in &payload.recipes {
            if recipe.identity.created_at > recipe.modified_at {
                return Err(CrispError::check(
                    ErrorCode::TimeOrder,
                    format!("recipe {} created_at is after modified_at", recipe.id.0),
                ));
            }
        }
        Ok(())
    });

    // D4 — every quantity carries a numeric `min` or a textual amount.
    checks.run("D4", || {
        for quantity in collect_quantities(payload) {
            if quantity.min.is_none() && quantity.text.is_none() {
                return Err(CrispError::check(
                    ErrorCode::Quantity,
                    "a quantity has neither a numeric min nor a text amount",
                ));
            }
        }
        Ok(())
    });
}

/// Checks that a set of localized-map keys is declared and includes the
/// bundle default language.
fn check_localized_keys<'a>(
    keys: impl Iterator<Item = &'a LanguageTag>,
    declared: &BTreeSet<&LanguageTag>,
    default_language: &LanguageTag,
) -> Result<()> {
    let mut present: BTreeSet<&LanguageTag> = BTreeSet::new();
    for key in keys {
        if !declared.contains(key) {
            return Err(CrispError::check(
                ErrorCode::L10nUndeclared,
                format!("localized text uses undeclared language {}", key.0),
            ));
        }
        present.insert(key);
    }
    if !present.contains(default_language) {
        return Err(CrispError::check(
            ErrorCode::L10nDefault,
            "a localized map is missing the bundle default language",
        ));
    }
    Ok(())
}

/// Phase E — signatures (collect-all). Returns the trust level established.
fn phase_e(checks: &mut Checks, decoded: &Decoded, platform_keys: &[PublicKeySet]) -> Trust {
    let cose = &decoded.cose;
    let payload = &decoded.payload;

    // Parse every signature's protected header.
    let mut parsed: Vec<(SigProtected, &crate::model::envelope::Signature)> = Vec::new();
    let mut parse_error: Option<CrispError> = None;
    for sig in &cose.signatures {
        match from_canonical_cbor::<SigProtected>(&sig.protected) {
            Ok(sp) => parsed.push((sp, sig)),
            Err(e) => parse_error = Some(e),
        }
    }
    let authors: Vec<_> = parsed
        .iter()
        .filter(|(sp, _)| sp.role == SigRole::Author)
        .collect();
    let endorsements: Vec<_> = parsed
        .iter()
        .filter(|(sp, _)| sp.role == SigRole::ServerEndorsement)
        .collect();

    // E1 — at least one author signature, each verifying.
    checks.run("E1", || {
        if let Some(e) = &parse_error {
            return Err(e.clone());
        }
        if authors.is_empty() {
            return Err(CrispError::check(
                ErrorCode::SigMissing,
                "no author signature is present",
            ));
        }
        for (sp, sig) in &authors {
            let keyset = author_keyset(payload, &sp.kid)?;
            let message = signing_message(&cose.protected, &sig.protected, &[])?;
            verify_composite(keyset, &message, &sig.sig_value).map_err(|_| {
                CrispError::check(ErrorCode::SigAuthor, "an author signature failed to verify")
            })?;
        }
        Ok(())
    });
    let author_ok = checks.passed("E1");

    // E2 — exactly one server endorsement, resolving to a trusted key.
    if platform_keys.is_empty() {
        checks.skip("E2");
    } else {
        checks.run("E2", || {
            if endorsements.len() != 1 {
                return Err(CrispError::check(
                    ErrorCode::SigMissing,
                    format!(
                        "expected exactly one server endorsement, found {}",
                        endorsements.len()
                    ),
                ));
            }
            let (sp, sig) = endorsements[0];
            let platform = platform_keys
                .iter()
                .find(|k| key_fingerprint(k).is_ok_and(|f| f.as_slice() == sp.kid.as_slice()))
                .ok_or_else(|| {
                    CrispError::check(ErrorCode::KeyUnknown, "endorsement key is not trusted")
                })?;
            let author_sig = cose.signatures.first().ok_or_else(|| {
                CrispError::check(ErrorCode::SigMissing, "no author signature to bind")
            })?;
            let external_aad = sha256(&to_canonical_cbor(author_sig)?);
            let message = signing_message(&cose.protected, &sig.protected, &external_aad)?;
            verify_composite(platform, &message, &sig.sig_value).map_err(|_| {
                CrispError::check(
                    ErrorCode::SigEndorsement,
                    "the server endorsement failed to verify",
                )
            })?;
            Ok(())
        });
    }
    let endorsed = checks.passed("E2");

    // E3 — modified_at is not after the endorsement time (plus skew).
    if let Some((sp, _)) = endorsements.first() {
        let limit = sp.signed_at.0.saturating_add(ENDORSEMENT_SKEW_SECS);
        checks.run("E3", || {
            if payload.meta.modified_at.0 > limit {
                return Err(CrispError::check(
                    ErrorCode::TimeOrder,
                    "meta.modified_at is after the endorsement time",
                ));
            }
            for recipe in &payload.recipes {
                if recipe.modified_at.0 > limit {
                    return Err(CrispError::check(
                        ErrorCode::TimeOrder,
                        format!(
                            "recipe {} modified_at is after the endorsement time",
                            recipe.id.0
                        ),
                    ));
                }
            }
            Ok(())
        });
    } else {
        checks.skip("E3");
    }

    if endorsed {
        Trust::Endorsed
    } else if author_ok {
        Trust::AuthorSigned
    } else {
        Trust::Unsigned
    }
}

/// Finds the author keyset whose fingerprint matches `kid`.
fn author_keyset<'a>(payload: &'a BundlePayload, kid: &[u8]) -> Result<&'a PublicKeySet> {
    for author in &payload.meta.authors {
        if key_fingerprint(&author.keyset)?.as_slice() == kid {
            return Ok(&author.keyset);
        }
    }
    Err(CrispError::check(
        ErrorCode::SigAuthor,
        "a signature kid matches no embedded author key",
    ))
}

/// Gathers every quantity reachable from the payload.
fn collect_quantities(payload: &BundlePayload) -> Vec<&Quantity> {
    let mut quantities = Vec::new();
    for recipe in &payload.recipes {
        quantities.push(&recipe.r#yield.amount);
        for times in recipe.times.iter() {
            quantities.extend(times.values());
        }
        for ingredient in &recipe.ingredients {
            if let Some(q) = &ingredient.quantity {
                quantities.push(q);
            }
            for substitute in ingredient.substitutes.iter().flatten() {
                if let Some(q) = &substitute.quantity {
                    quantities.push(q);
                }
            }
        }
        for step in &recipe.steps {
            if let Some(q) = &step.duration {
                quantities.push(q);
            }
        }
        if let Some(nutrition) = &recipe.nutrition {
            quantities.push(&nutrition.basis);
            quantities.extend(nutrition.values.values());
        }
    }
    quantities
}

/// Gathers every localized text and prose value reachable from the payload.
fn collect_localized(payload: &BundlePayload) -> (Vec<&LocalizedText>, Vec<&LocalizedProse>) {
    let mut texts: Vec<&LocalizedText> = vec![&payload.meta.title];
    let mut proses: Vec<&LocalizedProse> = Vec::new();
    if let Some(description) = &payload.meta.description {
        proses.push(description);
    }
    for recipe in &payload.recipes {
        collect_recipe_localized(recipe, &mut texts, &mut proses);
    }
    if let Some(book) = &payload.book {
        for part in &book.parts {
            texts.push(&part.title);
            for chapter in &part.chapters {
                texts.push(&chapter.title);
                if let Some(intro) = &chapter.intro {
                    proses.push(intro);
                }
                for entry in &chapter.entries {
                    if let Entry::Page { page } = entry {
                        texts.push(&page.title);
                        proses.push(&page.body);
                    }
                }
            }
        }
        for page in book
            .front_matter
            .iter()
            .flatten()
            .chain(book.back_matter.iter().flatten())
        {
            texts.push(&page.title);
            proses.push(&page.body);
        }
    }
    (texts, proses)
}

/// Collects the localized values within a single recipe.
fn collect_recipe_localized<'a>(
    recipe: &'a Recipe,
    texts: &mut Vec<&'a LocalizedText>,
    proses: &mut Vec<&'a LocalizedProse>,
) {
    texts.push(&recipe.title);
    texts.push(&recipe.r#yield.unit_text);
    texts.extend(recipe.ingredient_groups.iter().flatten());
    for prose in [&recipe.summary, &recipe.notes, &recipe.attribution]
        .into_iter()
        .flatten()
    {
        proses.push(prose);
    }
    for ingredient in &recipe.ingredients {
        texts.push(&ingredient.name);
        texts.extend(
            [&ingredient.display, &ingredient.preparation]
                .into_iter()
                .flatten(),
        );
        for substitute in ingredient.substitutes.iter().flatten() {
            texts.push(&substitute.name);
        }
    }
    for step in &recipe.steps {
        proses.push(&step.text);
        texts.extend([&step.group, &step.produces].into_iter().flatten());
    }
    for equipment in recipe.equipment.iter().flatten() {
        texts.push(&equipment.name);
        texts.extend(equipment.note.iter());
    }
}

/// Accumulates check results during verification.
struct Checks {
    results: Vec<CheckResult>,
}

impl Checks {
    fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    fn pass(&mut self, id: &str) {
        self.results.push(CheckResult {
            id: id.to_owned(),
            status: CheckStatus::Pass,
            code: None,
            detail: None,
        });
    }

    fn skip(&mut self, id: &str) {
        self.results.push(CheckResult {
            id: id.to_owned(),
            status: CheckStatus::Skipped,
            code: None,
            detail: None,
        });
    }

    fn fail(&mut self, id: &str, error: &CrispError) {
        self.results.push(CheckResult {
            id: id.to_owned(),
            status: CheckStatus::Fail,
            code: error.code().map(|c| c.as_str().to_owned()),
            detail: Some(error.to_string()),
        });
    }

    /// Runs a collect-all check, recording pass or fail.
    fn run(&mut self, id: &str, check: impl FnOnce() -> Result<()>) {
        match check() {
            Ok(()) => self.pass(id),
            Err(e) => self.fail(id, &e),
        }
    }

    /// Whether the recorded check `id` passed.
    fn passed(&self, id: &str) -> bool {
        self.results
            .iter()
            .any(|c| c.id == id && c.status == CheckStatus::Pass)
    }

    /// Whether any recorded check with an id beginning with `phase` failed.
    fn phase_failed(&self, phases: &[char]) -> bool {
        self.results.iter().any(|c| {
            c.status == CheckStatus::Fail
                && c.id.chars().next().is_some_and(|p| phases.contains(&p))
        })
    }

    /// Builds the final report, sorting checks by id.
    fn finish(
        mut self,
        format_version: u64,
        trust: Trust,
        verified_at: Timestamp,
    ) -> ComplianceReport {
        self.results.sort_by(|a, b| a.id.cmp(&b.id));
        let structural_ok = !self.phase_failed(&['A', 'B', 'C', 'D']);
        let signatures_ok = !self.phase_failed(&['E']);
        let class = if structural_ok && signatures_ok && trust == Trust::Endorsed {
            ComplianceClass::Published
        } else {
            ComplianceClass::Structural
        };
        let compliant = match class {
            ComplianceClass::Published => true,
            ComplianceClass::Structural => structural_ok,
        };
        ComplianceReport {
            crisp_report: CRISP_REPORT_MARKER,
            compliant,
            class,
            format_version,
            trust,
            verified_at,
            checks: self.results,
        }
    }
}

#[cfg(test)]
mod tests;
