//! Tool inputs and outputs: the JSON contract an MCP client sees.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::draft::{DraftIssue, RecipeDraft};
use crate::gateway::SearchHit;

/// `validate_recipe` input.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidateInput {
    /// The recipe to check.
    pub draft: RecipeDraft,
    /// The `doc_id` the draft will be saved `based_on`, if any, so the
    /// check matches what `save_recipe` will do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
}

/// `save_recipe` input.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveInput {
    /// The recipe to seal.
    pub draft: RecipeDraft,
    /// The `doc_id` this edits (from the library or the server). Your own
    /// recipe becomes a new revision; anyone else's becomes a fork crediting
    /// the original. Omit for a brand-new recipe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
    /// The `revision` `get_recipe` returned for `based_on`. When given, the
    /// save is refused if the recipe has changed since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<String>,
}

/// `list_recipes` input.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListInput {
    /// Case-insensitive text to match in titles, tags, and cuisines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

/// Input naming one document.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocInput {
    /// The recipe's `doc_id` (63 characters, starting `rcp1`).
    pub doc_id: String,
}

/// The vocabulary hints `crisp_status` reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Vocabulary {
    /// Registry course codes.
    pub course: Vec<String>,
    /// Registry diet codes.
    pub diet: Vec<String>,
    /// Registry allergen codes.
    pub allergens: Vec<String>,
    /// Unit words amounts may use.
    pub units: Vec<String>,
}

/// `crisp_status` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StatusOutput {
    /// The signing identity's `usr…` id, when it unlocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Why the identity cannot sign, when it cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_problem: Option<String>,
    /// The name recipes are signed under.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Where recipes are stored.
    pub library: String,
    /// The configured server, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// Codes and units drafts may use.
    pub vocabulary: Vocabulary,
}

/// `validate_recipe` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ValidateOutput {
    /// True when `save_recipe` would accept the draft.
    pub valid: bool,
    /// Every problem, each with a JSON Pointer into the draft.
    pub issues: Vec<DraftIssue>,
}

/// What a save made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SaveOutcome {
    /// A brand-new recipe.
    Created,
    /// A new revision of the signer's own recipe.
    Revised,
    /// A new recipe derived from someone else's.
    Forked,
}

/// `save_recipe` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SaveOutput {
    /// The stored recipe's `doc_id`.
    pub doc_id: String,
    /// Its new revision id.
    pub revision: String,
    /// What kind of save this was.
    pub outcome: SaveOutcome,
    /// Where the file is stored.
    pub library: String,
}

/// One recipe in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecipeSummary {
    /// Its `doc_id`.
    pub doc_id: String,
    /// Its title.
    pub title: String,
    /// Its current revision.
    pub revision: String,
    /// Last modified, seconds since the Unix epoch.
    pub modified_at: u64,
    /// Free tags.
    pub tags: Vec<String>,
    /// Cuisine slugs.
    pub cuisine: Vec<String>,
}

/// `list_recipes` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListOutput {
    /// The matching recipes, by title.
    pub recipes: Vec<RecipeSummary>,
    /// Files that could not be read, with the reason.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unreadable: Vec<String>,
}

/// `get_recipe` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetOutput {
    /// Its `doc_id`.
    pub doc_id: String,
    /// Its current revision.
    pub revision: String,
    /// `library` or `server`.
    pub found_in: String,
    /// Whether the file verifies offline: no check of phases A–E1 fails
    /// (structure and author signature; E2 and E3 need platform keys).
    pub verified: bool,
    /// The authors' display names.
    pub authors: Vec<String>,
    /// The recipe as an editable draft.
    pub draft: RecipeDraft,
    /// Content outside the draft, and what saving does with it.
    pub notices: Vec<String>,
}

/// `publish_recipe` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PublishOutput {
    /// The published `doc_id`.
    pub doc_id: String,
    /// The revision published.
    pub revision: String,
    /// False when a newer revision was saved while publishing, so the
    /// library kept that one instead of the endorsed copy.
    pub stored_endorsed: bool,
    /// The server it was published to.
    pub server: String,
}

/// `search_server` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchOutput {
    /// The published recipes found.
    pub results: Vec<SearchHit>,
}
