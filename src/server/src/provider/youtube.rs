use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use crate::provider::error::ProviderError;
use crate::provider::types::*;
use crate::provider::{LyricsProvider, ProviderSession};

pub struct YouTubeProvider {
    pub ytdlp_path: PathBuf,
    pub ytdlp_version: String,
}

impl YouTubeProvider {
    /// Returns `None` if yt-dlp cannot be found on PATH or fails to run.
    pub fn new() -> Option<Arc<Self>> {
        let path = which::which("yt-dlp").ok()?;
        let version = ytdlp_version(&path)?;
        Some(Arc::new(Self {
            ytdlp_path: path,
            ytdlp_version: version,
        }))
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

/// Run `yt-dlp --version` and return the version string, or `None` if it fails.
fn ytdlp_version(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        return None;
    }
    Some(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn ytdlp_version_returns_none_for_nonexistent_path() {
        let result = ytdlp_version(&PathBuf::from("/nonexistent/yt-dlp"));
        assert!(result.is_none());
    }

    #[test]
    fn ytdlp_version_returns_none_for_non_executable() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("yt-dlp");
        fs::write(&bin, "not an executable").unwrap();

        let result = ytdlp_version(&bin);
        assert!(result.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn ytdlp_version_parses_output_from_cmd_script() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-ytdlp.cmd");
        fs::write(&script, "@echo 2026.03.17\r\n").unwrap();

        let result = ytdlp_version(&script);
        assert_eq!(result.unwrap(), "2026.03.17");
    }

    #[cfg(not(windows))]
    #[test]
    fn ytdlp_version_parses_output_from_shell_script() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-ytdlp");
        fs::write(&script, "#!/bin/sh\necho 2026.03.17\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let result = ytdlp_version(&script);
        assert_eq!(result.unwrap(), "2026.03.17");
    }

    #[cfg(windows)]
    #[test]
    fn ytdlp_version_returns_none_for_failing_script() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("bad-ytdlp.cmd");
        fs::write(&script, "@exit /b 1\r\n").unwrap();

        let result = ytdlp_version(&script);
        assert!(result.is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn ytdlp_version_returns_none_for_failing_script() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("bad-ytdlp");
        fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let result = ytdlp_version(&script);
        assert!(result.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn ytdlp_version_returns_none_for_empty_output() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("empty-ytdlp.cmd");
        fs::write(&script, "@echo.\r\n").unwrap();

        let result = ytdlp_version(&script);
        // `echo.` outputs a blank line on Windows; trimmed to empty -> None
        assert!(result.is_none());
    }
}
