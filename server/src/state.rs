//! The shared application state threaded through every handler.

use std::sync::Arc;

use crate::config::Config;
use crate::platform::PlatformKey;
use crate::storage::Storage;

/// The state shared by every request handler.
///
/// It is cheap to clone — each field is an [`Arc`] — and is the single seam
/// through which handlers reach storage, the endorsement key, and config.
/// Handlers depend on the [`Storage`] *trait object*, so tests can swap in an
/// in-memory implementation without touching a database.
#[derive(Clone)]
pub struct AppState {
    /// The persistence layer.
    pub storage: Arc<dyn Storage>,
    /// The platform endorsement signing key.
    pub platform: Arc<PlatformKey>,
    /// Immutable runtime configuration.
    pub config: Arc<Config>,
}
