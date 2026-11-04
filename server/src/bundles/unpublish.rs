//! Unpublishing (`docs/public-server-policy.md` §3.1): a publisher removes
//! its own revisions of a document.
//!
//! Ownership is per revision (`published_by`), since any author of a
//! document may publish a revision of it: a co-author's revisions are never
//! removed, and while any remain the document stays served at the newest of
//! them. Deletion is hard and keeps no tombstone, so once no revision
//! remains the document answers 404 and any of its authors may publish it
//! again.

use kynos::prelude::*;

use super::{DocPath, RevisionPath};
use crate::auth::SessionToken;
use crate::error::{Forbidden, NotFound};
use crate::operation_error;
use crate::state::Store;
use crate::storage::Unpublish;

operation_error! {
    /// What the unpublish operations can fail with.
    pub enum UnpublishError { Forbidden, NotFound }
}

/// Answers an unpublish `outcome`: 204 when the caller's bundles were
/// removed, 404 when nothing matched, 403 when only other accounts'
/// bundles did. `what` names the target in the refusal.
fn answer(outcome: Unpublish, what: &str) -> Result<NoContent, UnpublishError> {
    match outcome {
        Unpublish::Removed(removed) => {
            tracing::info!(
                bundles = removed.bundles,
                bytes = removed.bytes,
                "unpublished"
            );
            Ok(NoContent)
        }
        Unpublish::Absent => {
            tracing::debug!("unpublish target not found");
            Err(NotFound::new(format!("no bundle with that {what}")).into())
        }
        Unpublish::NotOwner => {
            tracing::info!("unpublish refused: published by another account");
            Err(Forbidden::new(format!(
                "another account published the bundle with that {what}; only its publisher may remove it"
            ))
            .into())
        }
    }
}

/// Removes one revision the caller published.
///
/// Refused 403 when another account (a co-author) published it, and 404
/// when there is no such revision. If it was the newest, the next newest
/// remaining revision becomes the one `GET /bundles/{doc_id}` returns.
#[kynos::delete("/bundles/{doc_id}/{revision}")]
#[tracing::instrument(level = "info", skip_all, fields(usr_id, doc_id = %path.doc_id, revision = %path.revision))]
pub async fn unpublish_revision(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
    Path(path): Path<RevisionPath>,
) -> Result<NoContent, UnpublishError> {
    let caller = auth.into_inner().resolve()?;
    let usr_id = caller.account.usr_id;
    tracing::Span::current().record("usr_id", usr_id.as_str());
    let outcome = storage
        .unpublish(&usr_id, &path.doc_id, Some(&path.revision))
        .await?;
    answer(outcome, "document id and revision")
}

/// Removes every revision of a document that the caller published,
/// leaving revisions co-authors published.
///
/// Refused 403 when the caller published none of the document's revisions,
/// and 404 when the document has none. The document answers 404 only once
/// no revision remains.
#[kynos::delete("/bundles/{doc_id}")]
#[tracing::instrument(level = "info", skip_all, fields(usr_id, doc_id = %path.doc_id))]
pub async fn unpublish_document(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
    Path(path): Path<DocPath>,
) -> Result<NoContent, UnpublishError> {
    let caller = auth.into_inner().resolve()?;
    let usr_id = caller.account.usr_id;
    tracing::Span::current().record("usr_id", usr_id.as_str());
    let outcome = storage.unpublish(&usr_id, &path.doc_id, None).await?;
    answer(outcome, "document id")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::Usage;

    #[test]
    fn each_outcome_has_its_answer() {
        let removed = Unpublish::Removed(Usage {
            bytes: 1,
            bundles: 1,
        });
        assert!(answer(removed, "x").is_ok());
        assert!(matches!(
            answer(Unpublish::Absent, "x"),
            Err(UnpublishError::NotFound(_))
        ));
        assert!(matches!(
            answer(Unpublish::NotOwner, "x"),
            Err(UnpublishError::Forbidden(_))
        ));
    }
}
