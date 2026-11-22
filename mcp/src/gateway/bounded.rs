//! The bounds on every request to the server, and the transport under the
//! generated client that enforces them.
//!
//! The overall timeout travels on each request as the generated client's
//! total timeout. Every response body is capped: a body that declares more
//! than the cap is refused before any of it is read, and one that does not
//! declare its size is cut off at the first chunk that carries it past the
//! cap.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use crisp_core::limits::MAX_BUNDLE_SIZE;
use http_body::{Body, Frame, SizeHint};
use tracing::{debug, warn};

use super::GatewayError;
use crate::api::{self, ExecuteFuture, HttpBackend, HttpBody, TransportError};

/// How long one request to the server may take, from connecting until the
/// last byte of its response has arrived (2 minutes).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// The largest response body accepted from the server: the format's
/// whole-file limit ([`MAX_BUNDLE_SIZE`], 64 MiB), since the largest answer
/// the server legitimately sends is one bundle.
pub const MAX_RESPONSE_BYTES: u64 = MAX_BUNDLE_SIZE as u64;

/// The bounds on every request an [`super::HttpGateway`] makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The overall timeout of one request, connect through the last body
    /// byte.
    pub timeout: Duration,
    /// The largest response body accepted, in bytes.
    pub max_response_bytes: u64,
}

impl Default for Limits {
    /// [`REQUEST_TIMEOUT`] and [`MAX_RESPONSE_BYTES`].
    fn default() -> Self {
        Self {
            timeout: REQUEST_TIMEOUT,
            max_response_bytes: MAX_RESPONSE_BYTES,
        }
    }
}

impl Limits {
    /// A generated client for `url` whose every request is bounded by these
    /// limits.
    pub(super) fn client(&self, url: &str) -> Result<api::Client, GatewayError> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| GatewayError(format!("could not start the HTTP client: {e}")))?;
        let backend = BoundedBackend::new(
            Arc::new(api::ReqwestBackend::new(http)),
            self.max_response_bytes,
        );
        let client = api::Client::with_backend(Arc::new(backend), url)
            .map_err(|e| GatewayError(format!("CRISP_SERVER_URL is not a usable URL: {e}")))?
            .with_timeout(Some(self.timeout));
        debug!(
            timeout_ms = millis(self.timeout),
            max_response_bytes = self.max_response_bytes,
            "server requests are bounded"
        );
        Ok(client)
    }

    /// A call to `what`, started at `started`, that failed by passing one of
    /// these limits, described and logged by that limit; `None` for any
    /// other failure.
    pub(super) fn exceeded<E>(
        &self,
        what: &str,
        started: Instant,
        error: &api::Error<E>,
    ) -> Option<GatewayError> {
        if matches!(error, api::Error::Timeout(_)) {
            warn!(
                operation = what,
                timeout_ms = millis(self.timeout),
                elapsed_ms = millis(started.elapsed()),
                "the server did not answer in time"
            );
            return Some(GatewayError(format!(
                "{what}: the server did not finish answering within the {:?} request timeout",
                self.timeout
            )));
        }
        let refused = too_large(error)?;
        warn!(
            operation = what,
            limit_bytes = refused.limit,
            observed_bytes = refused.observed,
            declared = refused.declared,
            "refused an oversized response"
        );
        Some(GatewayError(format!("{what}: {refused}")))
    }
}

/// The oversized response a failed call was refused for, if it was.
fn too_large<E>(error: &api::Error<E>) -> Option<TooLarge> {
    let api::Error::Protocol(protocol) = error else {
        return None;
    };
    let mut cause = std::error::Error::source(protocol);
    while let Some(error) = cause {
        if let Some(found) = error.downcast_ref::<TooLarge>() {
            return Some(*found);
        }
        cause = error.source();
    }
    None
}

/// A duration's whole milliseconds, for a log field.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// A response body larger than the gateway accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooLarge {
    /// The cap, in bytes.
    pub limit: u64,
    /// The declared `Content-Length`, or the bytes read when the cap was
    /// passed.
    pub observed: u64,
    /// Whether `observed` is a declared length rather than bytes read.
    pub declared: bool,
}

impl std::fmt::Display for TooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            limit,
            observed,
            declared,
        } = self;
        write!(
            f,
            "the server's response is larger than the {limit}-byte limit "
        )?;
        if *declared {
            write!(f, "(it declared {observed} bytes)")
        } else {
            write!(f, "(cut off after {observed} bytes)")
        }
    }
}

impl std::error::Error for TooLarge {}

/// An [`HttpBackend`] that caps the response bodies of the one it wraps.
#[derive(Debug)]
struct BoundedBackend {
    inner: Arc<dyn HttpBackend>,
    limit: u64,
}

impl BoundedBackend {
    /// `inner`, with every response body capped at `limit` bytes.
    fn new(inner: Arc<dyn HttpBackend>, limit: u64) -> Self {
        Self { inner, limit }
    }
}

impl HttpBackend for BoundedBackend {
    fn execute(&self, request: http::Request<HttpBody>) -> ExecuteFuture<'_> {
        let path = request.uri().path().to_owned();
        let limit = self.limit;
        Box::pin(async move {
            let response = self.inner.execute(request).await?;
            let declared = response
                .headers()
                .get(http::header::CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok());
            if let Some(length) = declared.filter(|length| *length > limit) {
                debug!(
                    path,
                    limit,
                    declared = length,
                    "refusing a declared oversized response"
                );
                return Err(TransportError::protocol(TooLarge {
                    limit,
                    observed: length,
                    declared: true,
                }));
            }
            Ok(response.map(|body| {
                HttpBody::wrap(Capped {
                    inner: body,
                    limit,
                    read: 0,
                    path,
                })
            }))
        })
    }
}

/// A response body that fails once more than `limit` bytes have arrived.
struct Capped {
    inner: HttpBody,
    limit: u64,
    read: u64,
    path: String,
}

impl Body for Capped {
    type Data = Bytes;
    type Error = TransportError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, TransportError>>> {
        let this = self.get_mut();
        let polled = Pin::new(&mut this.inner).poll_frame(cx);
        if let Poll::Ready(Some(Ok(frame))) = &polled
            && let Some(data) = frame.data_ref()
        {
            this.read = this.read.saturating_add(data.len() as u64);
            if this.read > this.limit {
                debug!(
                    path = this.path,
                    limit = this.limit,
                    read = this.read,
                    "cutting off an oversized response"
                );
                return Poll::Ready(Some(Err(TransportError::protocol(TooLarge {
                    limit: this.limit,
                    observed: this.read,
                    declared: false,
                }))));
            }
        }
        polled
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
#[path = "bounded_tests.rs"]
mod tests;
