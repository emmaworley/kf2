//! Test-only mock provider used to drive the cache, eager-validate, and
//! lazy-refresh code paths end-to-end without any real network traffic.
//!
//! The `MockProvider` (factory side) hands back `MockProviderSession`s
//! (configured side). Its `configure` behavior and the session's call
//! behavior are controlled via shared atomics so tests can flip the
//! provider between "succeed", "fail once then succeed", "always fail",
//! and "token expired — fail once on first call then succeed".

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::provider::error::ProviderError;
use crate::provider::types::*;
use crate::provider::{LyricsProvider, ProviderSession, ScoringProvider, Searchable};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MockConfig {
    pub username: String,
    pub password: String,
}

impl TryFrom<&ProviderConfig> for MockConfig {
    type Error = serde_json::Error;

    fn try_from(value: &ProviderConfig) -> Result<Self, Self::Error> {
        serde_json::from_value(value.0.clone())
    }
}

impl From<MockConfig> for ProviderConfig {
    fn from(cfg: MockConfig) -> Self {
        ProviderConfig(serde_json::to_value(cfg).expect("MockConfig is always serializable"))
    }
}

#[derive(Default)]
pub struct MockProviderControl {
    /// If >0, the next N `configure` calls fail with AuthFailed before
    /// starting to succeed.
    pub configure_fail_count: AtomicUsize,
    /// If true, every `configure` call fails with AuthFailed.
    pub configure_always_fail: AtomicBool,
    /// If >0, the next N session data calls fail with AuthFailed before
    /// starting to succeed. Used to simulate token expiry.
    pub session_fail_count: AtomicUsize,
    /// Count of successful `configure` calls.
    pub configure_success_count: AtomicUsize,
}

pub struct MockProvider {
    pub metadata: ProviderMetadata,
    pub control: Arc<MockProviderControl>,
}

impl MockProvider {
    pub fn new(id: ProviderId) -> (Arc<Self>, Arc<MockProviderControl>) {
        let control = Arc::new(MockProviderControl::default());
        let provider = Arc::new(Self {
            metadata: ProviderMetadata {
                id,
                name: "Mock",
                capabilities: &[Capability::Search, Capability::Lyrics, Capability::Scoring],
                requires_configuration: true,
            },
            control: control.clone(),
        });
        (provider, control)
    }

    pub async fn configure(
        &self,
        config: Option<&ProviderConfig>,
    ) -> Result<Arc<dyn ProviderSession>, ProviderError> {
        if self.control.configure_always_fail.load(Ordering::SeqCst) {
            return Err(ProviderError::AuthFailed("mock: always-fail".into()));
        }
        let remaining = self.control.configure_fail_count.load(Ordering::SeqCst);
        if remaining > 0 {
            self.control
                .configure_fail_count
                .store(remaining - 1, Ordering::SeqCst);
            return Err(ProviderError::AuthFailed("mock: transient fail".into()));
        }
        let config =
            config.ok_or_else(|| ProviderError::AuthFailed("mock: config required".into()))?;
        let _cfg = MockConfig::try_from(config)
            .map_err(|e| ProviderError::AuthFailed(format!("mock: invalid config: {e}")))?;
        self.control
            .configure_success_count
            .fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(MockProviderSession {
            id: self.metadata.id,
            control: self.control.clone(),
        }))
    }
}

pub struct MockProviderSession {
    pub id: ProviderId,
    pub control: Arc<MockProviderControl>,
}

impl MockProviderSession {
    fn check_token(&self) -> Result<(), ProviderError> {
        let remaining = self.control.session_fail_count.load(Ordering::SeqCst);
        if remaining > 0 {
            self.control
                .session_fail_count
                .store(remaining - 1, Ordering::SeqCst);
            return Err(ProviderError::AuthFailed("mock: token expired".into()));
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl ProviderSession for MockProviderSession {
    async fn get_song(&self, song_id: &str) -> Result<Song, ProviderError> {
        self.check_token()?;
        Ok(Song {
            provider: self.id,
            id: song_id.to_string(),
            title: format!("mock song {song_id}"),
            artist: "mock artist".into(),
            duration: None,
            extra: SongExtra::Generic,
        })
    }

    async fn get_stream(&self, _song_id: &str) -> Result<MediaStream, ProviderError> {
        self.check_token()?;
        Ok(MediaStream::Hls {
            url_high: "https://mock/high.m3u8".into(),
            url_low: None,
        })
    }

    fn as_searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }

    fn as_lyrics_provider(&self) -> Option<&dyn LyricsProvider> {
        Some(self)
    }

    fn as_scoring_provider(&self) -> Option<&dyn ScoringProvider> {
        Some(self)
    }
}

#[tonic::async_trait]
impl Searchable for MockProviderSession {
    async fn search_songs(
        &self,
        query: &str,
        _page: u32,
    ) -> Result<SearchResults<SongResult>, ProviderError> {
        self.check_token()?;
        Ok(SearchResults {
            items: vec![SongResult {
                provider: self.id,
                id: "1".into(),
                title: format!("hit for {query}"),
                artist: "mock artist".into(),
            }],
            total_count: 1,
            has_more: false,
        })
    }

    async fn search_artists(
        &self,
        _query: &str,
        _page: u32,
    ) -> Result<SearchResults<Artist>, ProviderError> {
        self.check_token()?;
        Ok(SearchResults {
            items: vec![],
            total_count: 0,
            has_more: false,
        })
    }

    async fn songs_by_artist(
        &self,
        _artist_id: &str,
        _page: u32,
    ) -> Result<SearchResults<SongResult>, ProviderError> {
        self.check_token()?;
        Ok(SearchResults {
            items: vec![],
            total_count: 0,
            has_more: false,
        })
    }
}

#[tonic::async_trait]
impl LyricsProvider for MockProviderSession {
    async fn get_lyrics(&self, _song_id: &str) -> Result<Lyrics, ProviderError> {
        self.check_token()?;
        Ok(Lyrics::AdHoc("la la la".into()))
    }
}

#[tonic::async_trait]
impl ScoringProvider for MockProviderSession {
    async fn get_scoring(&self, _song_id: &str) -> Result<ScoringData, ProviderError> {
        self.check_token()?;
        Ok(ScoringData { notes: vec![] })
    }
}
