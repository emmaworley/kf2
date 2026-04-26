use std::sync::Arc;

use crate::provider::error::ProviderError;
use crate::provider::types::*;
use crate::provider::{LyricsProvider, ProviderSession};
use crate::tools::YtDlp;

pub struct YouTubeProvider {
    pub ytdlp: Arc<YtDlp>,
}

impl YouTubeProvider {
    pub fn new(ytdlp: Arc<YtDlp>) -> Arc<Self> {
        Arc::new(Self { ytdlp })
    }

    pub const METADATA: ProviderMetadata = ProviderMetadata {
        id: ProviderId::YouTube,
        name: "YouTube",
        capabilities: &[Capability::Lyrics],
        requires_configuration: false,
    };

    /// YouTube is stateless — `configure` hands back the same shared instance
    /// regardless of the (ignored) `config` argument.
    pub async fn configure(
        self: &Arc<Self>,
        _config: Option<&ProviderConfig>,
    ) -> Result<Arc<dyn ProviderSession>, ProviderError> {
        Ok(Arc::clone(self) as Arc<dyn ProviderSession>)
    }
}

#[tonic::async_trait]
impl ProviderSession for YouTubeProvider {
    async fn get_song(&self, _song_id: &str) -> Result<Song, ProviderError> {
        // TODO: implement real API call
        Err(ProviderError::NotSupported)
    }

    async fn get_stream(&self, _song_id: &str) -> Result<MediaStream, ProviderError> {
        // TODO: implement real download logic
        Err(ProviderError::NotSupported)
    }

    fn as_lyrics_provider(&self) -> Option<&dyn LyricsProvider> {
        Some(self)
    }
}

#[tonic::async_trait]
impl LyricsProvider for YouTubeProvider {
    async fn get_lyrics(&self, _song_id: &str) -> Result<Lyrics, ProviderError> {
        // TODO: implement real API call (fetch captions)
        Err(ProviderError::NotSupported)
    }
}
