//! Cache layer that turns a remote `Asset` into a `CachedAsset` backed by a
//! file (or HLS playlist directory) under a configurable cache root.
//!
//! Cache state has two shapes: "row + on-disk file" or "neither". A stale row
//! pointing at a missing file is treated as not-cached and rewritten the next
//! time `fetch` is called for that asset. Eviction is explicit; there is no
//! TTL or capacity bound.
//!
//! Only `MediaStream::Hls` sources are supported today. `DirectVideo` and
//! `YouTubeDownload` reach this layer as `UnsupportedSource` errors — they'll
//! grow remuxer/yt-dlp-driven pipelines in a follow-up.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::provider::types::{Asset, AssetId, MediaStream};
use crate::repo::{AssetCacheEntry, AssetCacheRepo, RepoError};
use crate::tools::hls::{self, HlsError, ProgressReporter};

#[derive(Debug, thiserror::Error)]
pub enum AssetCacheError {
    #[error(transparent)]
    Repo(#[from] RepoError),
    #[error(transparent)]
    Hls(#[from] HlsError),
    #[error("io error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("unsupported asset source: {0}")]
    UnsupportedSource(&'static str),
    #[error("malformed cache row for asset_id {asset_id}: {reason}")]
    MalformedRow { asset_id: String, reason: String },
}

/// Output of the asset cache: a stable URN identity together with the
/// absolute path to the local entry file. For HLS that's a rewritten
/// `playlist.m3u8` whose segments live in the same directory.
#[derive(Debug, Clone)]
pub struct CachedAsset {
    pub id: AssetId,
    pub local_path: PathBuf,
}

pub struct AssetCache {
    cache_dir: PathBuf,
    repo: Arc<dyn AssetCacheRepo>,
    http_client: reqwest::Client,
}

impl AssetCache {
    pub fn new(
        cache_dir: PathBuf,
        repo: Arc<dyn AssetCacheRepo>,
        http_client: reqwest::Client,
    ) -> Self {
        Self {
            cache_dir,
            repo,
            http_client,
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Resolve `asset` to a local file. If a row already exists and the file
    /// is on disk, return it untouched. Otherwise download (and write the row).
    pub async fn fetch(
        &self,
        asset: &Asset,
        progress: &dyn ProgressReporter,
    ) -> Result<CachedAsset, AssetCacheError> {
        if let Some(cached) = self.lookup_existing(&asset.id).await? {
            return Ok(cached);
        }
        self.download(asset, progress).await
    }

    /// Snapshot of the cache table, joined with absolute on-disk paths.
    /// Stale rows (DB row but missing file) are reported as-is — they'll be
    /// repaired the next time `fetch` runs for that asset.
    pub async fn list(&self) -> Result<Vec<CachedAsset>, AssetCacheError> {
        let rows = self.repo.list().await?;
        rows.into_iter()
            .map(|entry| self.entry_to_cached(entry))
            .collect()
    }

    /// Remove the row and the per-asset directory under `cache_dir`. Returns
    /// whether a DB row was actually deleted; the on-disk directory is best
    /// effort and missing directories are not an error.
    pub async fn evict(&self, id: &AssetId) -> Result<bool, AssetCacheError> {
        let removed = self.repo.delete(&id.to_string()).await?;
        let dir = self.cache_dir.join(asset_dir_name(id));
        match tokio::fs::remove_dir_all(&dir).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => return Err(AssetCacheError::Io { path: dir, source }),
        }
        Ok(removed)
    }

    async fn lookup_existing(&self, id: &AssetId) -> Result<Option<CachedAsset>, AssetCacheError> {
        let Some(entry) = self.repo.get(&id.to_string()).await? else {
            return Ok(None);
        };
        let cached = self.entry_to_cached(entry)?;
        match tokio::fs::try_exists(&cached.local_path).await {
            Ok(true) => Ok(Some(cached)),
            Ok(false) => {
                // Stale row: drop it so the next fetch redownloads cleanly.
                self.repo.delete(&id.to_string()).await?;
                Ok(None)
            }
            Err(source) => Err(AssetCacheError::Io {
                path: cached.local_path,
                source,
            }),
        }
    }

    async fn download(
        &self,
        asset: &Asset,
        progress: &dyn ProgressReporter,
    ) -> Result<CachedAsset, AssetCacheError> {
        let dir_name = asset_dir_name(&asset.id);
        let asset_dir = self.cache_dir.join(&dir_name);
        if let Err(source) = tokio::fs::create_dir_all(&asset_dir).await {
            return Err(AssetCacheError::Io {
                path: asset_dir,
                source,
            });
        }

        let entry_filename = match &asset.source {
            MediaStream::Hls { url_high, .. } => {
                let result =
                    hls::download_hls(&self.http_client, url_high, &asset_dir, progress).await?;
                result
                    .playlist_path
                    .file_name()
                    .expect("hls download_hls always returns a named playlist file")
                    .to_string_lossy()
                    .into_owned()
            }
            MediaStream::DirectVideo { .. } => {
                return Err(AssetCacheError::UnsupportedSource("direct_video"));
            }
            MediaStream::YouTubeDownload { .. } => {
                return Err(AssetCacheError::UnsupportedSource("youtube_download"));
            }
        };

        // Cross-platform-friendly relative path: store with forward slashes
        // so the same row works on either OS.
        let relative_path = format!("{dir_name}/{entry_filename}");
        self.repo
            .upsert(AssetCacheEntry {
                asset_id: asset.id.to_string(),
                relative_path: relative_path.clone(),
            })
            .await?;
        Ok(CachedAsset {
            id: asset.id.clone(),
            local_path: self.cache_dir.join(&dir_name).join(&entry_filename),
        })
    }

    fn entry_to_cached(&self, entry: AssetCacheEntry) -> Result<CachedAsset, AssetCacheError> {
        let id = entry
            .asset_id
            .parse::<AssetId>()
            .map_err(|e| AssetCacheError::MalformedRow {
                asset_id: entry.asset_id.clone(),
                reason: e.to_string(),
            })?;
        // `relative_path` is always written with forward slashes; on Windows
        // PathBuf::join still treats them correctly, but normalize via
        // explicit segment iteration so the resulting path is native.
        let mut local_path = self.cache_dir.clone();
        for seg in entry.relative_path.split('/') {
            local_path.push(seg);
        }
        Ok(CachedAsset { id, local_path })
    }
}

/// Directory name on disk for an asset. URN segments are alphanumeric/dash
/// by construction; replacing `:` with `_` is enough to get a portable name.
fn asset_dir_name(id: &AssetId) -> String {
    id.to_string().replace(':', "_")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::extract::State;
    use axum::http::header;
    use axum::response::IntoResponse;
    use axum::routing::get;
    use tempfile::TempDir;
    use tokio::net::TcpListener;

    use super::*;
    use crate::db;
    use crate::provider::types::{AssetId, MediaStream, ProviderId};
    use crate::repo::diesel_impl::DieselAssetCacheRepo;
    use crate::tools::hls::NoProgress;

    /// Hit counters for the mock playlist server, so tests can assert that
    /// the second `fetch` short-circuits.
    #[derive(Default)]
    struct ServerHits {
        playlist: AtomicUsize,
        segment: AtomicUsize,
    }

    async fn playlist_handler(State(hits): State<Arc<ServerHits>>) -> impl IntoResponse {
        hits.playlist.fetch_add(1, Ordering::SeqCst);
        let body = "#EXTM3U\n\
                    #EXT-X-VERSION:3\n\
                    #EXT-X-TARGETDURATION:10\n\
                    #EXT-X-MEDIA-SEQUENCE:0\n\
                    #EXTINF:10.0,\n\
                    seg_0.ts\n\
                    #EXT-X-ENDLIST\n";
        (
            [(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")],
            body,
        )
    }

    async fn segment_handler(State(hits): State<Arc<ServerHits>>) -> impl IntoResponse {
        hits.segment.fetch_add(1, Ordering::SeqCst);
        (
            [(header::CONTENT_TYPE, "video/mp2t")],
            &b"fake-ts-bytes"[..],
        )
    }

    /// Spin up a mini HLS server on an ephemeral port and return the playlist
    /// URL plus the hit counters. The server task is detached; the test
    /// scope ends before any cleanup matters.
    async fn spawn_hls_server() -> (String, Arc<ServerHits>) {
        let hits = Arc::new(ServerHits::default());
        let app = Router::new()
            .route("/playlist.m3u8", get(playlist_handler))
            .route("/seg_0.ts", get(segment_handler))
            .with_state(hits.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}/playlist.m3u8"), hits)
    }

    async fn cache_fixture(test_name: &str) -> (AssetCache, TempDir) {
        let pool = db::test_support::create_pool_in_memory(test_name)
            .await
            .unwrap();
        let repo: Arc<dyn AssetCacheRepo> = Arc::new(DieselAssetCacheRepo::new(pool));
        let cache_dir = tempfile::tempdir().unwrap();
        let cache = AssetCache::new(cache_dir.path().to_path_buf(), repo, reqwest::Client::new());
        (cache, cache_dir)
    }

    fn synthetic_asset(playlist_url: &str) -> Asset {
        Asset {
            id: AssetId::new(ProviderId::Dam, "contentsId:test123"),
            source: MediaStream::Hls {
                url_high: playlist_url.into(),
                url_low: None,
            },
        }
    }

    #[tokio::test]
    async fn fetch_downloads_then_short_circuits() {
        let (cache, _tmp) = cache_fixture("asset_cache_fetch_idempotent").await;
        let (url, hits) = spawn_hls_server().await;
        let asset = synthetic_asset(&url);

        let first = cache.fetch(&asset, &NoProgress).await.unwrap();
        assert_eq!(first.id, asset.id);
        assert!(first.local_path.exists(), "playlist file should be on disk");
        assert_eq!(hits.playlist.load(Ordering::SeqCst), 1);
        assert_eq!(hits.segment.load(Ordering::SeqCst), 1);

        // Second fetch must not re-download anything.
        let second = cache.fetch(&asset, &NoProgress).await.unwrap();
        assert_eq!(second.local_path, first.local_path);
        assert_eq!(hits.playlist.load(Ordering::SeqCst), 1);
        assert_eq!(hits.segment.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn fetch_repairs_stale_row_when_file_missing() {
        let (cache, _tmp) = cache_fixture("asset_cache_fetch_stale").await;
        let (url, hits) = spawn_hls_server().await;
        let asset = synthetic_asset(&url);

        let first = cache.fetch(&asset, &NoProgress).await.unwrap();
        // Simulate a user wiping the cache directory contents but leaving
        // the DB row behind.
        tokio::fs::remove_file(&first.local_path).await.unwrap();

        let recovered = cache.fetch(&asset, &NoProgress).await.unwrap();
        assert!(recovered.local_path.exists());
        assert_eq!(hits.playlist.load(Ordering::SeqCst), 2);
        assert_eq!(hits.segment.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn evict_removes_row_and_directory() {
        let (cache, tmp) = cache_fixture("asset_cache_evict").await;
        let (url, _hits) = spawn_hls_server().await;
        let asset = synthetic_asset(&url);

        let cached = cache.fetch(&asset, &NoProgress).await.unwrap();
        assert!(cached.local_path.exists());
        let dir = cached.local_path.parent().unwrap().to_path_buf();
        assert!(dir.starts_with(tmp.path()));
        assert!(dir.exists());

        let removed = cache.evict(&asset.id).await.unwrap();
        assert!(removed);
        assert!(!dir.exists(), "asset directory should be deleted");
        assert!(cache.list().await.unwrap().is_empty());

        // Idempotent second eviction.
        assert!(!cache.evict(&asset.id).await.unwrap());
    }

    #[tokio::test]
    async fn fetch_rejects_unsupported_sources() {
        let (cache, _tmp) = cache_fixture("asset_cache_unsupported").await;
        let asset = Asset {
            id: AssetId::new(ProviderId::YouTube, "videoId:dQw4w9WgXcQ"),
            source: MediaStream::YouTubeDownload {
                video_id: "dQw4w9WgXcQ".into(),
            },
        };
        let err = cache.fetch(&asset, &NoProgress).await.unwrap_err();
        assert!(matches!(
            err,
            AssetCacheError::UnsupportedSource("youtube_download")
        ));
    }
}
