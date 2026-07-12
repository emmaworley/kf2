use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum HlsError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("failed to parse m3u8 playlist: {0}")]
    Parse(String),

    #[error("unsupported encryption method: {0}")]
    UnsupportedEncryption(String),

    #[error("decryption failed for segment {segment}: {reason}")]
    Decryption { segment: String, reason: String },

    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}
