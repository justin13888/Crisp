//! The MCP surface: one tool per authoring operation, each traced in its
//! own span. The tool names are a contract the `crisp-skills` repository
//! builds on; rename one only together with the skills that call it.

mod ops;
mod publish;
mod views;

#[cfg(test)]
mod tests;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{Json, ServerHandler, tool, tool_handler, tool_router};
use tracing::{Instrument as _, info_span};

pub use ops::Authoring;
pub use views::*;

use crate::gateway::SearchQuery;

/// What the server tells a client about itself.
const INSTRUCTIONS: &str = "Crisp is an offline-first cookbook whose recipes are signed files. \
Use these tools to write recipes into the user's Crisp library: call crisp_status first, \
draft a recipe as JSON, run validate_recipe until it reports no issues, then save_recipe. \
To edit, get_recipe, change the draft (keeping each step's ref), and save_recipe with based_on \
set to its doc_id and expected_revision set to the revision get_recipe returned. Publish only \
when the \
user asks. Cite every source you relied on in `attribution` with https links, state \
verifiable facts in `notes`, and never present a recipe as the single best or only \
authentic version.";

/// The Crisp MCP server.
#[derive(Debug, Clone)]
pub struct CrispTools {
    authoring: Authoring,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl CrispTools {
    /// A server over `authoring`'s dependencies.
    #[must_use]
    pub fn new(authoring: Authoring) -> Self {
        Self {
            authoring,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Report the signing identity, library location, configured server, and the \
                       course/diet/allergen codes and unit words drafts may use. Call this first."
    )]
    async fn crisp_status(&self) -> Json<StatusOutput> {
        let span = info_span!("tool", name = "crisp_status");
        Json(self.authoring.status().instrument(span).await)
    }

    #[tool(
        description = "Check a recipe draft without saving it. Returns every problem with a JSON \
                       Pointer into the draft. Pass the same based_on you will save with. Fix \
                       them all, then call save_recipe."
    )]
    async fn validate_recipe(
        &self,
        Parameters(input): Parameters<ValidateInput>,
    ) -> Result<Json<ValidateOutput>, String> {
        let span = info_span!("tool", name = "validate_recipe");
        self.authoring
            .validate(&input.draft, input.based_on.as_deref())
            .instrument(span)
            .await
            .map(Json)
    }

    #[tool(
        description = "Sign a recipe draft with the user's key and store it in their library. \
                       With based_on, the save is a new revision of the user's own recipe, or a \
                       fork of someone else's. Returns the doc_id."
    )]
    async fn save_recipe(
        &self,
        Parameters(input): Parameters<SaveInput>,
    ) -> Result<Json<SaveOutput>, String> {
        let span = info_span!(
            "tool",
            name = "save_recipe",
            based_on = input.based_on.as_deref()
        );
        self.authoring
            .save(
                &input.draft,
                input.based_on.as_deref(),
                input.expected_revision.as_deref(),
            )
            .instrument(span)
            .await
            .map(Json)
    }

    #[tool(description = "List the recipes in the user's library, optionally matching a query.")]
    async fn list_recipes(
        &self,
        Parameters(input): Parameters<ListInput>,
    ) -> Result<Json<ListOutput>, String> {
        let _span = info_span!("tool", name = "list_recipes").entered();
        self.authoring.list(input.query.as_deref()).map(Json)
    }

    #[tool(
        description = "Read a recipe (from the library, else the server) as an editable draft, \
                       with notices about anything the draft does not show."
    )]
    async fn get_recipe(
        &self,
        Parameters(input): Parameters<DocInput>,
    ) -> Result<Json<GetOutput>, String> {
        let span = info_span!("tool", name = "get_recipe", doc_id = %input.doc_id);
        self.authoring
            .get(&input.doc_id)
            .instrument(span)
            .await
            .map(Json)
    }

    #[tool(
        description = "Publish a library recipe to the configured Crisp server, signed in as the \
                       user. Only call this when the user asks to publish."
    )]
    async fn publish_recipe(
        &self,
        Parameters(input): Parameters<DocInput>,
    ) -> Result<Json<PublishOutput>, String> {
        let span = info_span!("tool", name = "publish_recipe", doc_id = %input.doc_id);
        self.authoring
            .publish(&input.doc_id)
            .instrument(span)
            .await
            .map(Json)
    }

    #[tool(
        description = "Search recipes published on the configured Crisp server, e.g. to find \
                       existing versions of a dish before writing a new one."
    )]
    async fn search_server(
        &self,
        Parameters(query): Parameters<SearchQuery>,
    ) -> Result<Json<SearchOutput>, String> {
        let span = info_span!("tool", name = "search_server");
        self.authoring
            .gateway
            .search(query)
            .instrument(span)
            .await
            .map(|results| Json(SearchOutput { results }))
            .map_err(|e| e.0)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CrispTools {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("crisp-mcp", env!("CARGO_PKG_VERSION"))
                    .with_title("Crisp recipes"),
            )
            .with_instructions(INSTRUCTIONS)
    }
}
