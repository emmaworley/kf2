use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

pub struct FFmpeg {
    pub path: PathBuf,
    pub version: String,
}

impl FFmpeg {
    /// Returns `None` if ffmpeg cannot be found on PATH, fails to run, or
    /// produces output we can't parse a version out of.
    pub fn probe() -> Option<Arc<Self>> {
        let path = which::which("ffmpeg").ok()?;
        let version = probe_version(&path)?;
        Some(Arc::new(Self { path, version }))
    }
}

/// Run `ffmpeg -version` and pick out the version token from the first line.
///
/// `ffmpeg -version` prints something like:
///
/// ```text
/// ffmpeg version 6.1.1 Copyright (c) 2000-2023 the FFmpeg developers
/// built with ...
/// ```
///
/// We take the first whitespace-separated token after the literal `version`.
fn probe_version(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next()?.trim();
    let mut tokens = first_line.split_whitespace();
    while let Some(tok) = tokens.next() {
        if tok == "version" {
            let v = tokens.next()?.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn probe_version_returns_none_for_nonexistent_path() {
        let result = probe_version(&PathBuf::from("/nonexistent/ffmpeg"));
        assert!(result.is_none());
    }

    #[test]
    fn probe_version_returns_none_for_non_executable() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("ffmpeg");
        fs::write(&bin, "not an executable").unwrap();

        let result = probe_version(&bin);
        assert!(result.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn probe_version_parses_output_from_cmd_script() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-ffmpeg.cmd");
        fs::write(
            &script,
            "@echo ffmpeg version 6.1.1 Copyright (c) 2000-2023 the FFmpeg developers\r\n@echo built with foo\r\n",
        )
        .unwrap();

        let result = probe_version(&script);
        assert_eq!(result.unwrap(), "6.1.1");
    }

    #[cfg(not(windows))]
    #[test]
    fn probe_version_parses_output_from_shell_script() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-ffmpeg");
        fs::write(
            &script,
            "#!/bin/sh\necho 'ffmpeg version 6.1.1 Copyright (c) 2000-2023 the FFmpeg developers'\necho 'built with foo'\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let result = probe_version(&script);
        assert_eq!(result.unwrap(), "6.1.1");
    }

    #[cfg(windows)]
    #[test]
    fn probe_version_returns_none_for_failing_script() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("bad-ffmpeg.cmd");
        fs::write(&script, "@exit /b 1\r\n").unwrap();

        let result = probe_version(&script);
        assert!(result.is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn probe_version_returns_none_for_failing_script() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("bad-ffmpeg");
        fs::write(&script, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let result = probe_version(&script);
        assert!(result.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn probe_version_returns_none_for_unparseable_output() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("weird-ffmpeg.cmd");
        fs::write(&script, "@echo not the expected banner\r\n").unwrap();

        let result = probe_version(&script);
        assert!(result.is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn probe_version_returns_none_for_unparseable_output() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("weird-ffmpeg");
        fs::write(&script, "#!/bin/sh\necho 'not the expected banner'\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

        let result = probe_version(&script);
        assert!(result.is_none());
    }
}
