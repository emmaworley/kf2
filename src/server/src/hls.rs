pub mod download;
pub mod error;
pub mod parse;
pub mod progress;
pub mod types;

pub use error::HlsError;
pub use progress::{HlsEvent, NoProgress, ProgressReporter};
pub use types::DownloadResult;

use std::path::Path;

pub async fn download_hls(
    client: &reqwest::Client,
    playlist_url: &str,
    output_dir: &Path,
    progress: &dyn ProgressReporter,
) -> Result<DownloadResult, HlsError> {
    let url =
        url::Url::parse(playlist_url).map_err(|e| HlsError::Parse(format!("invalid URL: {e}")))?;

    let body = client
        .get(url.clone())
        .send()
        .await?
        .error_for_status()?
        .text()
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
