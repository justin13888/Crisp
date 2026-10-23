//! Phase C — integrity & identity (spec §11.1, C1–C6).

use std::collections::BTreeSet;

use crate::crypto::bech32id::{Hrp, decode_id};
use crate::crypto::hash::sha256;
use crate::error::ErrorCode;
use crate::identity::{
    derive_blob_id, derive_book_id, derive_recipe_id, derive_revision, derive_user_id,
    key_fingerprint,
};
use crate::model::book::Entry;
use crate::model::ids::BlobId;
use crate::model::payload::{BundlePayload, Kind};
use crate::model::primitives::OpenCode;
use crate::model::recipe::Recipe;
use crate::model::text::{LocalizedProse, RunRefKind, RunTarget};
use crate::verify::issue::Issues;
use crate::verify::walk::{self, ProseScope, Visitor};

/// C1 — each blob is content-addressed, correctly sized, and allowlisted.
pub(crate) fn blobs(payload: &BundlePayload) -> Issues {
    let mut issues = Issues::default();
    for (id, blob) in &payload.blobs {
        let path = format!("/blobs/{}", id.0);
        let size_ok = usize::try_from(blob.size).is_ok_and(|size| size == blob.bytes.len());
        issues.require(
            size_ok,
            &format!("{path}/size"),
            ErrorCode::BlobSize,
            || {
                format!(
                    "declares {} bytes but holds {}",
                    blob.size,
                    blob.bytes.len()
                )
            },
        );
        let hash_ok = derive_blob_id(&blob.bytes).is_ok_and(|derived| derived == *id);
        issues.require(hash_ok, &path, ErrorCode::BlobHash, || {
            "the blob id does not match the SHA-256 of its bytes".to_owned()
        });
        if let OpenCode::Other(media_type) = &blob.media_type {
            issues.push(
                &format!("{path}/media_type"),
                ErrorCode::BlobMediaType,
                format!("{media_type:?} is not an allowlisted raster image type"),
            );
        }
    }
    issues
}

/// C2 — every identifier is valid bech32m under the HRP its field requires.
pub(crate) fn ids(payload: &BundlePayload) -> Issues {
    struct Ids(Issues);
    impl<'a> Visitor<'a> for Ids {
        fn id(&mut self, path: &str, id: &'a str, hrp: Hrp) {
            match decode_id(id) {
                Ok((actual, _)) if actual == hrp => {}
                Ok((actual, _)) => self.0.push(
                    path,
                    ErrorCode::IdChecksum,
                    format!(
                        "expected a {}… id, found {}…",
                        hrp.as_str(),
                        actual.as_str()
                    ),
                ),
                Err(e) => self.0.push(path, ErrorCode::IdChecksum, e.to_string()),
            }
        }
        fn blob_ref(&mut self, path: &str, blob: &'a BlobId) {
            self.id(path, &blob.0, Hrp::Blob);
        }
        fn prose(&mut self, path: &str, prose: &'a LocalizedProse, _scope: ProseScope<'a>) {
            for_each_run_ref(path, prose, |rp, kind, target| {
                if let (RunRefKind::Recipe, RunTarget::Recipe(id)) = (kind, target) {
                    self.id(rp, &id.0, Hrp::Recipe);
                }
            });
        }
    }
    let mut v = Ids(Issues::default());
    walk::walk(payload, &mut v);
    v.0
}

/// C3 — ids and revisions derive from their content, and `doc_id` names the
/// document.
pub(crate) fn derivations(payload: &BundlePayload) -> Issues {
    let mut issues = Issues::default();
    for (i, recipe) in payload.recipes.iter().enumerate() {
        let rp = format!("/recipes/{i}");
        let id_ok = derive_recipe_id(&recipe.identity).is_ok_and(|id| id == recipe.id);
        issues.require(id_ok, &format!("{rp}/id"), ErrorCode::IdMismatch, || {
            "the id does not derive from the recipe's identity".to_owned()
        });
        let rev_ok = derive_revision(recipe).is_ok_and(|rev| rev == recipe.revision);
        issues.require(
            rev_ok,
            &format!("{rp}/revision"),
            ErrorCode::RevMismatch,
            || "the revision does not match the recipe's content".to_owned(),
        );
        issues.require(
            recipe.supersedes.as_ref() != Some(&recipe.revision),
            &format!("{rp}/supersedes"),
            ErrorCode::RevMismatch,
            || "a revision cannot supersede itself".to_owned(),
        );
    }
    let expected = match payload.kind {
        Kind::Recipe => payload.recipes.first().map(|r| r.id.0.clone()),
        Kind::Book => payload
            .meta
            .identity
            .as_ref()
            .and_then(|identity| derive_book_id(identity).ok())
            .map(|id| id.0),
    };
    issues.require(
        expected.as_deref() == Some(payload.doc_id.0.as_str()),
        "/doc_id",
        ErrorCode::IdMismatch,
        || "doc_id does not name this document".to_owned(),
    );
    issues
}

/// C4 — the protected header's content hash equals `SHA-256(payload)`.
pub(crate) fn content_hash(payload_bytes: &[u8], declared: &[u8]) -> Issues {
    let mut issues = Issues::default();
    issues.require(
        sha256(payload_bytes).as_slice() == declared,
        "",
        ErrorCode::ContentHash,
        || "the protected header's content hash does not match the payload".to_owned(),
    );
    issues
}

/// C5 — every author id derives from its key, and every identity's
/// `author_fpr` belongs to a listed author.
pub(crate) fn author_binding(payload: &BundlePayload) -> Issues {
    let mut issues = Issues::default();
    let mut fingerprints = BTreeSet::new();
    for (i, author) in payload.meta.authors.iter().enumerate() {
        if let Ok(fpr) = key_fingerprint(&author.keyset) {
            fingerprints.insert(fpr.to_vec());
        }
        let uid_ok = derive_user_id(&author.keyset).is_ok_and(|uid| uid == author.uid);
        issues.require(
            uid_ok,
            &format!("/meta/authors/{i}/uid"),
            ErrorCode::AuthorBinding,
            || "the author id does not derive from the author's keyset".to_owned(),
        );
    }
    let identities = payload
        .recipes
        .iter()
        .enumerate()
        .map(|(i, r)| (format!("/recipes/{i}/identity/author_fpr"), &r.identity))
        .chain(
            payload
                .meta
                .identity
                .iter()
                .map(|identity| ("/meta/identity/author_fpr".to_owned(), identity)),
        );
    for (path, identity) in identities {
        issues.require(
            fingerprints.contains(&identity.author_fpr),
            &path,
            ErrorCode::AuthorBinding,
            || "author_fpr matches no listed author's key".to_owned(),
        );
    }
    issues
}

/// C6 — handles are unique and every internal reference resolves.
pub(crate) fn references(payload: &BundlePayload) -> Issues {
    struct Refs<'a> {
        payload: &'a BundlePayload,
        issues: Issues,
    }
    impl<'a> Visitor<'a> for Refs<'a> {
        fn recipe(&mut self, path: &str, _index: usize, recipe: &'a Recipe) {
            recipe_refs(path, recipe, &mut self.issues);
        }
        fn blob_ref(&mut self, path: &str, blob: &'a BlobId) {
            self.issues.require(
                self.payload.blobs.contains_key(blob),
                path,
                ErrorCode::Ref,
                || format!("blob {} is not in the blob store", blob.0),
            );
        }
        fn prose(&mut self, path: &str, prose: &'a LocalizedProse, scope: ProseScope<'a>) {
            let payload = self.payload;
            let issues = &mut self.issues;
            for_each_run_ref(path, prose, |rp, kind, target| {
                let ok = match (kind, target, scope) {
                    (RunRefKind::Recipe, RunTarget::Recipe(id), _) => {
                        payload.recipes.iter().any(|r| r.id == *id)
                    }
                    (RunRefKind::Ingredient, RunTarget::Handle(h), Some(r)) => {
                        r.ingredients.iter().any(|i| i.r#ref == *h)
                    }
                    (RunRefKind::Step, RunTarget::Handle(h), Some(r)) => {
                        r.steps.iter().any(|s| s.r#ref == *h)
                    }
                    (RunRefKind::Equipment, RunTarget::Handle(h), Some(r)) => {
                        r.equipment.iter().flatten().any(|e| e.r#ref == *h)
                    }
                    _ => false,
                };
                issues.require(ok, rp, ErrorCode::Ref, || {
                    "the run reference does not resolve".to_owned()
                });
            });
        }
    }
    let mut v = Refs {
        payload,
        issues: Issues::default(),
    };
    walk::walk(payload, &mut v);

    if let Some(book) = &payload.book {
        for (pi, part) in book.parts.iter().enumerate() {
            for (ci, chapter) in part.chapters.iter().enumerate() {
                for (ei, entry) in chapter.entries.iter().enumerate() {
                    if let Entry::Recipe { recipe } = entry {
                        let ok = usize::try_from(*recipe).is_ok_and(|i| i < payload.recipes.len());
                        v.issues.require(
                            ok,
                            &format!("/book/parts/{pi}/chapters/{ci}/entries/{ei}/recipe"),
                            ErrorCode::Ref,
                            || format!("recipe index {recipe} is out of range"),
                        );
                    }
                }
            }
        }
    }
    v.issues
}

/// Checks handle uniqueness and the step → ingredient/equipment/group links
/// inside one recipe.
fn recipe_refs(rp: &str, recipe: &Recipe, issues: &mut Issues) {
    let ingredients = unique(
        recipe.ingredients.iter().map(|i| i.r#ref),
        &format!("{rp}/ingredients"),
        issues,
    );
    let equipment = unique(
        recipe.equipment.iter().flatten().map(|e| e.r#ref),
        &format!("{rp}/equipment"),
        issues,
    );
    unique(
        recipe.steps.iter().map(|s| s.r#ref),
        &format!("{rp}/steps"),
        issues,
    );

    let ingredient_groups = recipe.ingredient_groups.as_ref().map_or(0, Vec::len);
    for (i, ing) in recipe.ingredients.iter().enumerate() {
        if let Some(group) = ing.group {
            in_range(
                group,
                ingredient_groups,
                &format!("{rp}/ingredients/{i}/group"),
                issues,
            );
        }
    }
    let step_groups = recipe.step_groups.as_ref().map_or(0, Vec::len);
    for (i, step) in recipe.steps.iter().enumerate() {
        let sp = format!("{rp}/steps/{i}");
        if let Some(group) = step.group {
            in_range(group, step_groups, &format!("{sp}/group"), issues);
        }
        for (ui, used) in step.ingredients.iter().flatten().enumerate() {
            issues.require(
                ingredients.contains(&used.r#ref),
                &format!("{sp}/ingredients/{ui}/ref"),
                ErrorCode::Ref,
                || format!("no ingredient has ref {}", used.r#ref),
            );
        }
        for (ei, handle) in step.equipment.iter().flatten().enumerate() {
            issues.require(
                equipment.contains(handle),
                &format!("{sp}/equipment/{ei}"),
                ErrorCode::Ref,
                || format!("no equipment has ref {handle}"),
            );
        }
    }
}

/// Collects handles, reporting any duplicate.
fn unique(handles: impl Iterator<Item = u64>, path: &str, issues: &mut Issues) -> BTreeSet<u64> {
    let mut seen = BTreeSet::new();
    for (i, handle) in handles.enumerate() {
        issues.require(
            seen.insert(handle),
            &format!("{path}/{i}/ref"),
            ErrorCode::Ref,
            || format!("ref {handle} is used twice"),
        );
    }
    seen
}

fn in_range(index: u64, len: usize, path: &str, issues: &mut Issues) {
    let ok = usize::try_from(index).is_ok_and(|i| i < len);
    issues.require(ok, path, ErrorCode::Ref, || {
        format!("group index {index} is out of range")
    });
}

/// Calls `f` with the path, kind, and target of every run reference.
pub(crate) fn for_each_run_ref<'a>(
    path: &str,
    prose: &'a LocalizedProse,
    mut f: impl FnMut(&str, RunRefKind, &'a RunTarget),
) {
    for (lang, body) in &prose.0 {
        for (pi, paragraph) in body.0.iter().enumerate() {
            for (ri, run) in paragraph.0.iter().enumerate() {
                if let Some(reference) = &run.reference {
                    f(
                        &format!("{path}/{}/{pi}/{ri}/ref", lang.0),
                        reference.kind,
                        &reference.target,
                    );
                }
            }
        }
    }
}
