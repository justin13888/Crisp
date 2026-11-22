//! What each tool does, over injected dependencies. Errors are sentences
//! an LLM can act on; they become the tool's error result.

use std::sync::Arc;

use crisp_core::author::{open_draft, seal, validate_draft};
use crisp_core::model::payload::BundlePayload;
use crisp_core::model::recipe::{AllergenCode, CourseCode, DietCode};
use crisp_core::render::units::accepted_words;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use super::verdict::{verdict, verified_base};
use super::views::{
    GetOutput, ListOutput, RecipeSummary, SaveOutcome, SaveOutput, StatusOutput, ValidateOutput,
    Vocabulary,
};
use crate::clock::Clock;
use crate::draft::{DraftIssue, RecipeDraft, from_payload, reroot, to_payload};
use crate::gateway::Gateway;
use crate::library::Library;
use crate::signer::Signer;

/// The tools' dependencies.
#[derive(Clone)]
pub struct Authoring {
    /// Serializes library writes: a save, or a publish's write-back.
    pub(crate) writes: Arc<Mutex<()>>,
    /// Where recipes are stored.
    pub library: Arc<dyn Library>,
    /// The Crisp server.
    pub gateway: Arc<dyn Gateway>,
    /// The signing identity.
    pub signer: Arc<Signer>,
    /// The name recipes are signed under.
    pub display_name: Option<String>,
    /// The time source.
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for Authoring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Authoring")
            .field("library", &self.library.location())
            .field("server", &self.gateway.url())
            .finish_non_exhaustive()
    }
}

fn wire_codes<T: serde::Serialize>(codes: &[T]) -> Vec<String> {
    codes
        .iter()
        .filter_map(|c| serde_json::to_value(c).ok()?.as_str().map(str::to_owned))
        .collect()
}

/// Every problem with a draft: conversion first, then the format's checks.
fn issues_of(
    draft: &RecipeDraft,
    base: Option<&BundlePayload>,
) -> Result<BundlePayload, Vec<DraftIssue>> {
    let payload = to_payload(draft, base)?;
    let issues: Vec<DraftIssue> = validate_draft(&payload)
        .into_iter()
        .map(|issue| DraftIssue {
            path: reroot(&issue.path),
            code: Some(issue.code.to_string()),
            message: issue.message,
        })
        .collect();
    if issues.is_empty() {
        Ok(payload)
    } else {
        Err(issues)
    }
}

fn describe(issues: &[DraftIssue]) -> String {
    let lines: Vec<String> = issues
        .iter()
        .map(|i| {
            format!(
                "- {}: {}",
                if i.path.is_empty() {
                    "(draft)"
                } else {
                    &i.path
                },
                i.message
            )
        })
        .collect();
    format!(
        "the draft was not saved; fix these and save again (validate_recipe lists them too):\n{}",
        lines.join("\n")
    )
}

/// A file's document id and revision.
pub(super) fn identify(bytes: &[u8]) -> Result<(String, String), String> {
    let payload = open_draft(bytes).map_err(|e| e.to_string())?;
    let revision = payload
        .recipes
        .first()
        .map(|r| r.revision.0.clone())
        .unwrap_or_default();
    Ok((payload.doc_id.0, revision))
}

/// Refuses a copy that is not the document asked for.
fn same_document(doc_id: &str, actual: &str, found_in: &str) -> Result<(), String> {
    if actual == doc_id {
        Ok(())
    } else {
        Err(format!(
            "the {found_in} copy of {doc_id} is a different document ({actual}); not using it"
        ))
    }
}

impl Authoring {
    /// Dependencies for the tools.
    #[must_use]
    pub fn new(
        library: Arc<dyn Library>,
        gateway: Arc<dyn Gateway>,
        signer: Arc<Signer>,
        display_name: Option<String>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            writes: Arc::new(Mutex::new(())),
            library,
            gateway,
            signer,
            display_name,
            clock,
        }
    }

    /// `crisp_status`.
    pub async fn status(&self) -> StatusOutput {
        let (user_id, signer_problem) = match self.signer.unlock().await.and_then(|s| {
            s.user_id()
                .map_err(|e| crate::signer::SignerError(e.to_string()))
        }) {
            Ok(uid) => (Some(uid.0), None),
            Err(e) => (None, Some(e.0)),
        };
        StatusOutput {
            user_id,
            signer_problem,
            display_name: self.display_name.clone(),
            library: self.library.location(),
            server: self.gateway.url(),
            vocabulary: Vocabulary {
                course: wire_codes(CourseCode::ALL),
                diet: wire_codes(DietCode::ALL),
                allergens: wire_codes(AllergenCode::ALL),
                // Catalogue words the amount parser converts and scales;
                // any other word (`3 cloves`) is a counted annotation.
                units: accepted_words().into_iter().map(str::to_owned).collect(),
            },
        }
    }

    /// `validate_recipe`: the checks `save_recipe` runs, against the same
    /// base when `based_on` is given.
    ///
    /// # Errors
    ///
    /// Returns a message when `based_on` cannot be found or opened.
    pub async fn validate(
        &self,
        draft: &RecipeDraft,
        based_on: Option<&str>,
    ) -> Result<ValidateOutput, String> {
        let base = match based_on {
            Some(id) => Some(self.base(id).await?.1),
            None => None,
        };
        let issues = issues_of(draft, base.as_ref()).err().unwrap_or_default();
        debug!(issues = issues.len(), "draft validated");
        Ok(ValidateOutput {
            valid: issues.is_empty(),
            issues,
        })
    }

    /// The file a save edits, and its payload. Whichever copy is found, it
    /// must pass the A–E pipeline before anything else reads it.
    async fn base(&self, doc_id: &str) -> Result<(Vec<u8>, BundlePayload), String> {
        let (bytes, found_in) = self.locate(doc_id).await?;
        let bytes = verified_base(doc_id, found_in, bytes, self.clock.now()).await?;
        let payload = open_draft(&bytes)
            .map_err(|e| format!("could not open the recipe being edited: {e}"))?;
        same_document(doc_id, &payload.doc_id.0, found_in)?;
        Ok((bytes, payload))
    }

    /// A stored file: the library's copy, else the server's. Either must be
    /// the document asked for.
    async fn find(&self, doc_id: &str) -> Result<(Vec<u8>, &'static str), String> {
        let (bytes, found_in) = self.locate(doc_id).await?;
        let (actual, _) = identify(&bytes).map_err(|e| format!("{doc_id} does not open: {e}"))?;
        same_document(doc_id, &actual, found_in)?;
        Ok((bytes, found_in))
    }

    /// A stored file as found, unchecked: the library's copy, else the
    /// server's.
    async fn locate(&self, doc_id: &str) -> Result<(Vec<u8>, &'static str), String> {
        let found = match self.library.read(doc_id).map_err(|e| e.0)? {
            Some(bytes) => Some((bytes, "library")),
            None if self.gateway.url().is_some() => self
                .gateway
                .download(doc_id)
                .await
                .map_err(|e| e.0)?
                .map(|bytes| (bytes, "server")),
            None => None,
        };
        found.ok_or_else(|| {
            format!(
                "no recipe {doc_id} in the library{}",
                if self.gateway.url().is_some() {
                    " or on the server"
                } else {
                    ""
                }
            )
        })
    }

    /// `save_recipe`.
    ///
    /// # Errors
    ///
    /// Returns what to fix: draft problems, a missing base, or a signer that
    /// cannot sign.
    pub async fn save(
        &self,
        draft: &RecipeDraft,
        based_on: Option<&str>,
        expected_revision: Option<&str>,
    ) -> Result<SaveOutput, String> {
        // The base is fetched (possibly downloaded) before taking the lock.
        let (previous, base) = match based_on {
            Some(id) => {
                let (bytes, payload) = self.base(id).await?;
                (Some(bytes), Some(payload))
            }
            None => (None, None),
        };
        let base_revision = base
            .as_ref()
            .and_then(|b| b.recipes.first())
            .map(|r| r.revision.0.clone());
        if expected_revision.is_some() && based_on.is_none() {
            return Err("expected_revision only applies to an edit: also pass based_on".into());
        }
        if let (Some(expected), Some(actual)) = (expected_revision, &base_revision)
            && expected != actual
        {
            return Err(format!(
                "the recipe changed since you read it (now revision {actual}); call get_recipe \
                 again and reapply your edit"
            ));
        }
        // One save at a time; and refuse when the library's copy moved on
        // since the base was read, so no revision silently drops another.
        let _writing = self.writes.lock().await;
        if let (Some(id), Some(read)) = (based_on, &base_revision)
            && let Some(current) = self.library.read(id).map_err(|e| e.0)?
        {
            let (_, now) = identify(&current)?;
            if &now != read {
                return Err(format!(
                    "{id} was saved again while this edit was in progress (now revision {now}); \
                     call get_recipe again and reapply your edit"
                ));
            }
        }
        let payload = issues_of(draft, base.as_ref()).map_err(|i| describe(&i))?;
        let display_name = self
            .display_name
            .as_deref()
            .ok_or("set CRISP_DISPLAY_NAME to the name recipes should be signed under")?;
        let secret = self.signer.unlock().await.map_err(|e| e.0)?;
        let bytes = seal(
            &payload,
            &secret,
            display_name,
            self.clock.now(),
            previous.as_deref(),
        )
        .map_err(|e| format!("sealing failed: {e}"))?;
        let sealed =
            open_draft(&bytes).map_err(|e| format!("the sealed file does not reopen: {e}"))?;
        let recipe = &sealed.recipes[0];
        let outcome = match (&base, &recipe.supersedes) {
            (None, _) => SaveOutcome::Created,
            (Some(_), Some(_)) => SaveOutcome::Revised,
            (Some(_), None) => SaveOutcome::Forked,
        };
        self.library
            .write(&sealed.doc_id.0, &bytes)
            .map_err(|e| e.0)?;
        info!(doc_id = %sealed.doc_id.0, revision = %recipe.revision.0, ?outcome, "recipe saved");
        Ok(SaveOutput {
            doc_id: sealed.doc_id.0.clone(),
            revision: recipe.revision.0.clone(),
            outcome,
            library: self.library.location(),
        })
    }

    /// `list_recipes`.
    ///
    /// # Errors
    ///
    /// Returns a message when the library cannot be listed.
    pub fn list(&self, query: Option<&str>) -> Result<ListOutput, String> {
        let needle = query
            .map(str::to_lowercase)
            .filter(|q| !q.trim().is_empty());
        let mut recipes = Vec::new();
        let mut unreadable = Vec::new();
        for id in self.library.list().map_err(|e| e.0)? {
            let opened = self
                .library
                .read(&id)
                .map_err(|e| e.0)
                .and_then(|b| b.ok_or_else(|| "vanished while listing".to_owned()))
                .and_then(|b| open_draft(&b).map_err(|e| e.to_string()))
                .and_then(|p| from_payload(&p).map(|x| (p, x)));
            let (payload, extracted) = match opened {
                Ok(found) => found,
                Err(reason) => {
                    warn!(doc_id = %id, %reason, "unreadable library file");
                    unreadable.push(format!("{id}: {reason}"));
                    continue;
                }
            };
            let d = extracted.draft;
            let haystack =
                format!("{} {} {}", d.title, d.tags.join(" "), d.cuisine.join(" ")).to_lowercase();
            if needle
                .as_ref()
                .is_some_and(|n| !haystack.contains(n.trim()))
            {
                continue;
            }
            recipes.push(RecipeSummary {
                doc_id: id,
                title: d.title,
                revision: payload.recipes[0].revision.0.clone(),
                modified_at: payload.recipes[0].modified_at.0,
                tags: d.tags,
                cuisine: d.cuisine,
            });
        }
        recipes.sort_by_key(|r| r.title.to_lowercase());
        Ok(ListOutput {
            recipes,
            unreadable,
        })
    }

    /// `get_recipe`.
    ///
    /// # Errors
    ///
    /// Returns a message when the recipe is not found or cannot be opened.
    pub async fn get(&self, doc_id: &str) -> Result<GetOutput, String> {
        let (bytes, found_in) = self.find(doc_id).await?;
        let subject = format!("the {found_in} copy of {doc_id}");
        let (bytes, verdict) = verdict(&subject, bytes, self.clock.now()).await?;
        let verified = verdict.passes();
        if !verified {
            warn!(doc_id, found_in, reasons = %verdict.reasons(), "recipe read does not verify");
        }
        let payload = open_draft(&bytes).map_err(|e| format!("{doc_id} does not open: {e}"))?;
        let extracted = from_payload(&payload)?;
        Ok(GetOutput {
            doc_id: payload.doc_id.0.clone(),
            revision: payload.recipes[0].revision.0.clone(),
            found_in: found_in.to_owned(),
            verified,
            authors: payload
                .meta
                .authors
                .iter()
                .map(|a| a.display_name.clone())
                .collect(),
            draft: extracted.draft,
            notices: extracted.notices,
        })
    }
}
