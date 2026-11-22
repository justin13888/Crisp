//! Every tool against an in-memory library, a scripted server, and a fixed
//! clock.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use crisp_core::author::{open_draft, seal};
use crisp_core::model::primitives::{LanguageTag, Timestamp};
use crisp_core::render::parse_amount;
use crisp_core::sign::SecretKeySet;
use rmcp::handler::server::wrapper::Parameters;

use super::*;
use crate::clock::FixedClock;
use crate::draft::{RecipeDraft, to_payload};
use crate::gateway::{Gateway, GatewayError, NoServer, SearchHit};
use crate::library::{Library, MemoryLibrary};
use crate::signer::Signer;

const NOW: Timestamp = Timestamp(1_750_000_000);

/// A scripted server: downloads come from `files`, publishes are echoed
/// back (optionally replaced by `publish_returns`), searches return `hits`.
#[derive(Default)]
struct MockServer {
    files: Mutex<Vec<(String, Vec<u8>)>>,
    publish_returns: Option<Vec<u8>>,
    published: Mutex<Vec<Vec<u8>>>,
    hits: Vec<SearchHit>,
    /// Written into this library mid-publish, as a concurrent save would.
    saved_during_publish: Option<(Arc<MemoryLibrary>, String, Vec<u8>)>,
}

#[async_trait]
impl Gateway for MockServer {
    fn url(&self) -> Option<String> {
        Some("https://crisp.test".into())
    }

    async fn publish(&self, bundle: Vec<u8>) -> Result<Vec<u8>, GatewayError> {
        self.published.lock().expect("lock").push(bundle.clone());
        if let Some((library, doc_id, newer)) = &self.saved_during_publish {
            library.write(doc_id, newer).expect("concurrent save");
        }
        Ok(self.publish_returns.clone().unwrap_or(bundle))
    }

    async fn download(&self, doc_id: &str) -> Result<Option<Vec<u8>>, GatewayError> {
        Ok(self
            .files
            .lock()
            .expect("lock")
            .iter()
            .find(|(id, _)| id == doc_id)
            .map(|(_, b)| b.clone()))
    }

    async fn search(&self, _query: SearchQuery) -> Result<Vec<SearchHit>, GatewayError> {
        Ok(self.hits.clone())
    }
}

fn me() -> SecretKeySet {
    SecretKeySet::from_seed(&[7u8; 32])
}

fn someone_else() -> SecretKeySet {
    SecretKeySet::from_seed(&[8u8; 32])
}

fn authoring(library: Arc<MemoryLibrary>, gateway: Arc<dyn Gateway>) -> Authoring {
    Authoring::new(
        library,
        gateway,
        Arc::new(Signer::from_secret(me())),
        Some("Me".into()),
        Arc::new(FixedClock(NOW)),
    )
}

fn local() -> (Arc<MemoryLibrary>, Authoring) {
    let library = Arc::new(MemoryLibrary::default());
    (Arc::clone(&library), authoring(library, Arc::new(NoServer)))
}

fn sample() -> RecipeDraft {
    crate::draft::tests::sample()
}

/// A file sealed by someone else, as if downloaded or shared.
fn foreign_file() -> (String, Vec<u8>) {
    let payload = to_payload(&sample(), None).expect("converts");
    let bytes = seal(
        &payload,
        &someone_else(),
        "Other",
        Timestamp(1_700_000_000),
        None,
    )
    .expect("seals");
    (open_draft(&bytes).expect("opens").doc_id.0, bytes)
}

mod authoring;
mod serving;

#[test]
fn debug_output_names_the_library_and_server() {
    let (library, tools) = local();
    let shown = format!("{tools:?}");
    assert!(
        shown.starts_with("Authoring") && shown.contains(&library.location()),
        "{shown}"
    );
    assert!(shown.contains("server: None"), "{shown}");
}
