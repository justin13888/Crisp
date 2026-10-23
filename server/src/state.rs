//! The application context threaded through every handler.
//!
//! [`App`] is the kynos dependency-injection context: `#[derive(Provider)]`
//! lets a handler ask for any field by type with `Inject<T>`. Each field is a
//! cheap-to-clone newtype over an [`Arc`], so injection is a refcount bump,
//! and each wraps a distinct type so lookup-by-type is unambiguous.

use std::sync::Arc;

use kynos::Provider;

use crate::auth::SessionAuthenticator;
use crate::config::Config;
use crate::platform::PlatformKey;
use crate::storage::Storage;

/// The persistence layer, as a trait object so tests can substitute an
/// in-memory implementation without touching a database.
#[derive(Clone)]
pub struct Store(pub Arc<dyn Storage>);

impl std::ops::Deref for Store {
    type Target = dyn Storage;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// The platform endorsement key.
#[derive(Clone, Debug)]
pub struct Platform(pub Arc<PlatformKey>);

impl std::ops::Deref for Platform {
    type Target = PlatformKey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Immutable runtime configuration.
#[derive(Clone, Debug)]
pub struct Settings(pub Arc<Config>);

impl std::ops::Deref for Settings {
    type Target = Config;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// The context every request handler is served with.
#[derive(Provider)]
pub struct App {
    /// The persistence layer.
    pub storage: Store,
    /// The platform endorsement signing key.
    pub platform: Platform,
    /// Immutable runtime configuration.
    pub settings: Settings,
    /// Resolves bearer tokens to accounts; reached through
    /// [`kynos::security::Authenticates`], not injection.
    #[provide(skip)]
    pub sessions: SessionAuthenticator,
}

impl App {
    /// Assembles the context from its parts.
    #[must_use]
    pub fn new(storage: Arc<dyn Storage>, platform: Arc<PlatformKey>, config: Arc<Config>) -> Self {
        Self {
            storage: Store(storage),
            platform: Platform(platform),
            settings: Settings(config),
            sessions: SessionAuthenticator,
        }
    }
}
