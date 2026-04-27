pub mod download;
pub mod error;
pub mod parse;
pub mod progress;
pub mod types;

pub use error::HlsError;
pub use progress::{HlsEvent, NoProgress, ProgressReporter};
pub use types::DownloadResult;

use std::path::Path;
use std::time::Duration;

use backon::{ExponentialBuilder, Retryable};
use reqwest::Error;

pub async fn download_hls(
    client: &reqwest::Client,
    playlist_url: &str,
    output_dir: &Path,
    progress: &dyn ProgressReporter,
) -> Result<DownloadResult, HlsError> {
    let url =
        url::Url::parse(playlist_url).map_err(|e| HlsError::Parse(format!("invalid URL: {e}")))?;

    // The playlist GET is the most frequent failure point — DAM-style CDNs
    // 403 transiently when the freshly-minted signed URL is hit too quickly.
    // Per-request retry covers the transient case; if the URL is genuinely
    // one-time-use and stays burned, callers re-mint via `provider.get_asset`
    // and call `download_hls` again.
    let body = (|| async {
        client
            .get(url.clone())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await
    })
    .retry(playlist_backoff())
    .when(is_retryable)
    .await?;

    let playlist = parse::parse_playlist(&url, &body)?;

    tokio::fs::create_dir_all(output_dir)
        .await
        .map_err(|e| HlsError::Io {
            path: output_dir.to_path_buf(),
            source: e,
        })?;

    download::download_playlist(client, &playlist, output_dir, progress).await
}

fn playlist_backoff() -> ExponentialBuilder {
    ExponentialBuilder::default()
        .with_max_times(4)
        .with_min_delay(Duration::from_secs(1))
        .with_max_delay(Duration::from_secs(5))
        .with_jitter()
}

fn is_retryable(e: &Error) -> bool {
    if e.is_timeout() || e.is_connect() {
        return true;
    }
    match e.status() {
        Some(s) => s.is_server_error() || matches!(s.as_u16(), 403 | 408 | 429),
        None => false,
    }
}
