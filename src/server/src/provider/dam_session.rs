use crate::provider::error::ProviderError;
use crate::provider::types::*;
use crate::provider::{ProviderSession, ScoringProvider, Searchable};

pub struct DamProviderSession {
    #[allow(dead_code)]
    pub(crate) client: reqwest::Client,
    #[allow(dead_code)]
    pub(crate) token: String,
}

impl DamProviderSession {
    pub fn new(client: reqwest::Client, token: String) -> Self {
        Self { client, token }
    }
}

#[tonic::async_trait]
impl ProviderSession for DamProviderSession {
    async fn get_song(&self, _song_id: &str) -> Result<Song, ProviderError> {
        // TODO: implement real API call
        Err(ProviderError::NotSupported)
    }

    async fn get_stream(&self, _song_id: &str) -> Result<MediaStream, ProviderError> {
        // TODO: implement real API call
        Err(ProviderError::NotSupported)
    }

    fn as_searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }

    fn as_scoring_provider(&self) -> Option<&dyn ScoringProvider> {
        Some(self)
    }
}

#[tonic::async_trait]
impl Searchable for DamProviderSession {
    async fn search_songs(
        &self,
        _query: &str,
        _page: u32,
    ) -> Result<SearchResults<Song>, ProviderError> {
        // TODO: implement real API call
        Err(ProviderError::NotSupported)
    }

    async fn search_artists(
        &self,
        _query: &str,
        _page: u32,
    ) -> Result<SearchResults<Artist>, ProviderError> {
        // TODO: implement real API call
        Err(ProviderError::NotSupported)
    }

    async fn songs_by_artist(
        &self,
        _artist_id: &str,
        _page: u32,
    ) -> Result<SearchResults<Song>, ProviderError> {
        // TODO: implement real API call
        Err(ProviderError::NotSupported)
    }
}

#[tonic::async_trait]
impl ScoringProvider for DamProviderSession {
    async fn get_scoring(&self, _song_id: &str) -> Result<ScoringData, ProviderError> {
        // TODO: implement real API call
        Err(ProviderError::NotSupported)
    }
}
