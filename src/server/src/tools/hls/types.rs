use std::path::PathBuf;

#[derive(Debug)]
pub struct Playlist {
    pub target_duration: Option<u64>,
    pub media_sequence: u64,
    pub segments: Vec<Segment>,
    pub end_list: bool,
}

#[derive(Debug)]
pub struct Segment {
    pub duration: f64,
    pub url: String,
    pub byte_range: Option<ByteRange>,
    pub encryption: EncryptionState,
    pub init_segment: Option<InitSegment>,
    pub discontinuity: bool,
}

#[derive(Debug, Clone)]
pub struct ByteRange {
    pub length: u64,
    pub offset: u64,
}

#[derive(Debug, Clone)]
pub enum EncryptionState {
    None,
    Aes128 {
        key_url: String,
        iv: Option<[u8; 16]>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InitSegment {
    pub url: String,
    pub byte_range: Option<InitByteRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InitByteRange {
    pub length: u64,
    pub offset: u64,
}

#[derive(Debug)]
pub struct DownloadResult {
    pub playlist_path: PathBuf,
    pub segment_count: usize,
}
