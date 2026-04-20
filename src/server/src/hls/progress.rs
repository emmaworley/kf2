pub enum HlsEvent<'a> {
    /// Fired once after the top-level playlist is parsed and the output
    /// directory is created, before any segment is downloaded.
    PlaylistParsed { segment_count: usize },
    /// Fired after a segment (index 0-based) has been downloaded, decrypted,
    /// and written to disk.
    SegmentComplete { index: usize, total: usize },
    /// Fired after each new init segment is written to disk. Most streams
    /// have one init segment total; some have a handful. Reporters that only
    /// care about playable-segment progress can ignore this.
    InitSegmentComplete { url: &'a str },
}

pub trait ProgressReporter: Send + Sync {
    fn report(&self, event: HlsEvent<'_>);
}

pub struct NoProgress;

impl ProgressReporter for NoProgress {
    fn report(&self, _: HlsEvent<'_>) {}
}
