use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

pub struct YtDlp {
    pub path: PathBuf,
    pub version: String,
}

impl YtDlp {
    /// Returns `None` if yt-dlp cannot be found on PATH or fails to run.
    pub fn probe() -> Option<Arc<Self>> {
        let path = which::which("yt-dlp").ok()?;
        let version = probe_version(&path)?;
        Some(Arc::new(Self { path, version }))
    }
}

/// Run `yt-dlp --version` and return the version string, or `None` if it fails.
fn probe_version(path: &Path) -> Option<String> {
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
    fn probe_version_returns_none_for_nonexistent_path() {
        let result = probe_version(&PathBuf::from("/nonexistent/yt-dlp"));
        assert!(result.is_none());
    }

    #[test]
    fn probe_version_returns_none_for_non_executable() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("yt-dlp");
        fs::write(&bin, "not an executable").unwrap();

        let result = probe_version(&bin);
        assert!(result.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn probe_version_parses_output_from_cmd_script() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-ytdlp.cmd");
        fs::write(&script, "@echo 2026.03.17\r\n").unwrap();

        let result = probe_version(&script);
        assert_eq!(result.unwrap(), "2026.03.17");
    }

    #[cfg(not(windows))]
    #[test]
    fn probe_version_parses_output_from_shell_script() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-ytdlp");
        fs::write(&script, "#!/bin/sh\necho 2026.03.17\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let result = probe_version(&script);
        assert_eq!(result.unwrap(), "2026.03.17");
    }

    #[cfg(windows)]
    #[test]
    fn probe_version_returns_none_for_failing_script() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("bad-ytdlp.cmd");
        fs::write(&script, "@exit /b 1\r\n").unwrap();

        let result = probe_version(&script);
        assert!(result.is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn probe_version_returns_none_for_failing_script() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("bad-ytdlp");
        fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let result = probe_version(&script);
        assert!(result.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn probe_version_returns_none_for_empty_output() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("empty-ytdlp.cmd");
        fs::write(&script, "@echo.\r\n").unwrap();

        let result = probe_version(&script);
        // `echo.` outputs a blank line on Windows; trimmed to empty -> None
        assert!(result.is_none());
    }
}
