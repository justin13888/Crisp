//! The `request` span every API request runs inside: an event a handler
//! emits carries the id `RequestId` assigned the request, the same one the
//! response returns as `X-Request-Id`.

mod common;

use std::io;
use std::sync::{Arc, Mutex};

use common::{Fault, Harness};
use kynos::http::StatusCode;
use serde_json::Value;
use tracing_subscriber::fmt::MakeWriter;

/// Collects every line the subscriber writes.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("capture lock")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Captured {
    type Writer = Self;

    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

#[tokio::test]
async fn an_event_inside_a_handler_carries_the_request_id() {
    let captured = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .with_span_list(true)
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(captured.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    // A storage failure inside `get_exact` is logged at ERROR by `DbFailure`,
    // from within the handler.
    let h = Harness::new();
    h.storage.inject("bundle_exact", Fault::Db);
    let response = h
        .client
        .get("/api/v1/bundles/doc1span/rev1span")
        .send()
        .await;
    response.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
    let request_id = response
        .header("x-request-id")
        .expect("the response carries its request id")
        .to_owned();

    let output = String::from_utf8(captured.0.lock().expect("capture lock").clone())
        .expect("the log is UTF-8");
    let failure = output
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|event| event["level"] == "ERROR")
        .unwrap_or_else(|| panic!("the handler logged its failure: {output}"));
    let spans = failure["spans"]
        .as_array()
        .expect("the event lists its spans");
    let request = spans
        .iter()
        .find(|span| span["name"] == "request")
        .unwrap_or_else(|| panic!("the failure is inside the request span: {failure}"));
    assert_eq!(request["request_id"], request_id.as_str());
    assert!(
        spans.iter().any(|span| span["name"] == "get_exact"),
        "the handler's own span nests inside the request span: {failure}"
    );
}
