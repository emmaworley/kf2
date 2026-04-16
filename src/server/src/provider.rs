pub mod cache;
pub mod dam;
pub mod error;
pub mod joysound;
pub mod joysound_session;
#[cfg(any(test, feature = "test-support"))]
pub mod mock;
pub mod types;
pub mod youtube;

use std::collections::HashMap;
use std::sync::Arc;

use error::ProviderError;
use types::{
    Artist, Lyrics, MediaStream, ProviderConfig, ProviderId, ProviderMetadata, ScoringData,
    SearchResults, Song, SongResult,
};

use crate::provider::dam::DamProvider;
use crate::provider::joysound::JoysoundProvider;
use crate::provider::youtube::YouTubeProvider;

// ---------------------------------------------------------------------------
// Session trait — what every per-(KF2-session, provider) authed handle must
// implement. `Provider::configure` returns `Arc<dyn ProviderSession>`, and
// callers dispatch through this trait. Optional capabilities (search, lyrics,
// scoring) are exposed via `as_*` downcasts that default to `None`.
// ---------------------------------------------------------------------------

#[tonic::async_trait]
pub trait ProviderSession: Send + Sync {
    async fn get_song(&self, song_id: &str) -> Result<Song, ProviderError>;
    async fn get_stream(&self, song_id: &str) -> Result<MediaStream, ProviderError>;

    fn as_searchable(&self) -> Option<&dyn Searchable> {
        None
    }
    fn as_lyrics_provider(&self) -> Option<&dyn LyricsProvider> {
        None
    }
    fn as_scoring_provider(&self) -> Option<&dyn ScoringProvider> {
        None
    }
}

// ---------------------------------------------------------------------------
// Capability traits — implemented by sessions that support the given
// operation. Wired up through `ProviderSession::as_*` so callers can
// optimistically downcast.
// ---------------------------------------------------------------------------

/// Providers that support keyword search.
#[tonic::async_trait]
pub trait Searchable: Send + Sync {
    async fn search_songs(
        &self,
        query: &str,
        page: u32,
    ) -> Result<SearchResults<SongResult>, ProviderError>;
    async fn search_artists(
        &self,
        query: &str,
        page: u32,
    ) -> Result<SearchResults<Artist>, ProviderError>;
    async fn songs_by_artist(
        &self,
        artist_id: &str,
        page: u32,
    ) -> Result<SearchResults<SongResult>, ProviderError>;
}

/// Providers that can return lyrics data.
#[tonic::async_trait]
pub trait LyricsProvider: Send + Sync {
    async fn get_lyrics(&self, song_id: &str) -> Result<Lyrics, ProviderError>;
}

/// Providers that have pitch/scoring data (piano roll).
#[tonic::async_trait]
pub trait ScoringProvider: Send + Sync {
    async fn get_scoring(&self, song_id: &str) -> Result<ScoringData, ProviderError>;
}

// ---------------------------------------------------------------------------
// Provider enum — the static/registry side. One `Provider` per provider type
// per server, shared across all sessions. Holds no per-session state.
// ---------------------------------------------------------------------------

pub enum Provider {
    Dam(DamProvider),
    Joysound(JoysoundProvider),
    YouTube(Arc<YouTubeProvider>),
    #[cfg(any(test, feature = "test-support"))]
    Mock(Arc<mock::MockProvider>),
}

impl Provider {
    pub fn metadata(&self) -> &ProviderMetadata {
        match self {
            Provider::Dam(_) => &DamProvider::METADATA,
            Provider::Joysound(_) => &JoysoundProvider::METADATA,
            Provider::YouTube(_) => &YouTubeProvider::METADATA,
            #[cfg(any(test, feature = "test-support"))]
            Provider::Mock(p) => &p.metadata,
        }
    }

    /// Produce a `ProviderSession` for this `Provider`, authenticating against
    /// the upstream API using the given `ProviderConfig` if necessary.
    /// Providers whose `metadata().requires_configuration` is false accept
    /// `None` and reuse their shared, stateless instance.
    pub async fn configure(
        &self,
        config: Option<&ProviderConfig>,
    ) -> Result<Arc<dyn ProviderSession>, ProviderError> {
        match self {
            Provider::Dam(p) => p.configure(config).await,
            Provider::Joysound(p) => p.configure(config).await,
            Provider::YouTube(p) => p.configure(config).await,
            #[cfg(any(test, feature = "test-support"))]
            Provider::Mock(p) => p.configure(config).await,
        }
    }

    /// Adapter between the proto schema (flat `username`/`password`) and each
    /// provider's typed config. Stateless providers return a `null` envelope.
    // TODO: delete once `ConfigureProviderRequest` gains per-provider oneofs.
    pub fn build_config_from_basic_auth(
        &self,
        username: String,
        password: String,
    ) -> ProviderConfig {
        match self {
            Provider::Dam(_) => dam::DamConfig { username, password }.into(),
            Provider::Joysound(_) => joysound::JoysoundConfig { username, password }.into(),
            Provider::YouTube(_) => ProviderConfig(serde_json::Value::Null),
            #[cfg(any(test, feature = "test-support"))]
            Provider::Mock(_) => mock::MockConfig { username, password }.into(),
        }
    }

    /// Best-effort extraction of a display username from a stored config,
    /// dispatched to the owning provider. Returns `None` for providers that
    /// don't have a username (e.g. YouTube) or whose config doesn't parse.
    pub fn username_from_config(&self, config: &ProviderConfig) -> Option<String> {
        match self {
            Provider::Dam(_) => dam::DamConfig::try_from(config).ok().map(|c| c.username),
            Provider::Joysound(_) => joysound::JoysoundConfig::try_from(config)
                .ok()
                .map(|c| c.username),
            Provider::YouTube(_) => None,
            #[cfg(any(test, feature = "test-support"))]
            Provider::Mock(_) => mock::MockConfig::try_from(config).ok().map(|c| c.username),
        }
    }
}

// ---------------------------------------------------------------------------
// Provider registry
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct ProviderRegistry {
    providers: HashMap<ProviderId, Arc<Provider>>,
}

impl ProviderRegistry {
    pub fn register(&mut self, provider: Arc<Provider>) {
        let id = provider.metadata().id;
        self.providers.insert(id, provider);
    }

    pub fn get(&self, id: ProviderId) -> Option<&Arc<Provider>> {
        self.providers.get(&id)
    }

    pub fn all(&self) -> impl Iterator<Item = &Arc<Provider>> {
        self.providers.values()
    }
}
