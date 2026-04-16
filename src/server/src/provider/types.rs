use std::time::Duration;

/// Identifies which provider a piece of data came from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display, strum::EnumString, strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum ProviderId {
    Dam,
    Joysound,
    YouTube,
}

impl ProviderId {
    pub fn as_str(&self) -> &'static str {
        self.into()
    }
}

/// Opaque per-session provider config envelope. Persisted as a JSON blob in
/// `session_provider_config.config_json`. Each provider module defines its
/// own strongly-typed config struct and converts via `From`/`TryFrom`; this
/// type is what crosses the repo/factory boundary so adding a new provider
/// doesn't touch any central enum.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ProviderConfig(pub serde_json::Value);

/// A capability that a provider may support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Search,
    Lyrics,
    Scoring,
}

/// Static, per-provider-type description: identity, display name, supported
/// capabilities, and whether the provider needs per-session credentials.
#[derive(Debug, Clone, Copy)]
pub struct ProviderMetadata {
    pub id: ProviderId,
    pub name: &'static str,
    pub capabilities: &'static [Capability],
    pub requires_configuration: bool,
}

/// A song as returned from any provider.
#[derive(Debug, Clone)]
pub struct Song {
    pub provider: ProviderId,
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration: Option<Duration>,
    pub extra: SongExtra,
}

/// A song as returned from a search endpoint. Carries only the fields
/// providers actually populate in list results — resolve to a full `Song`
/// via `ProviderSession::get_song` when `duration`/`extra` are needed.
#[derive(Debug, Clone)]
pub struct SongResult {
    pub provider: ProviderId,
    pub id: String,
    pub title: String,
    pub artist: String,
}

/// Provider-specific song metadata.
#[derive(Debug, Clone)]
pub enum SongExtra {
    /// No provider-specific metadata. Used by tests/fixtures and providers
    /// that don't carry any extra fields.
    Generic,
    Dam(DamSongExtra),
    Joysound(JoysoundSongExtra),
    YouTube(YouTubeSongExtra),
}

#[derive(Debug, Clone, Default)]
pub struct DamSongExtra {
    pub vocal_types: Vec<DamVocalType>,
    pub has_scoring: bool,
    pub score_level: u32,
    pub technical_level: u32,
    pub shift: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamVocalType {
    Normal,
    GuideMale,
    GuideFemale,
}

#[derive(Debug, Clone)]
pub struct JoysoundSongExtra {
    pub lyricist: Option<String>,
    pub composer: Option<String>,
    pub reading: Option<String>,
    pub fadeout_time: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct YouTubeSongExtra {
    pub channel: Option<String>,
    pub description: Option<String>,
    pub view_count: Option<u64>,
    pub caption_languages: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Artist {
    pub provider: ProviderId,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SearchResults<T> {
    pub items: Vec<T>,
    pub total_count: u32,
    pub has_more: bool,
}

/// Lyrics come in different shapes depending on the provider.
#[derive(Debug, Clone)]
pub enum Lyrics {
    /// DAM: lyrics are embedded in the stream, nothing to return separately.
    PreRendered,
    /// Joysound: raw telop data the client must render.
    Telop(TelopData),
    /// YouTube/other: timed text captions.
    Captions(Vec<CaptionTrack>),
    /// User-provided ad-hoc lyrics (plain text).
    AdHoc(String),
}

#[derive(Debug, Clone)]
pub struct TelopData {
    pub segments: Vec<TelopSegment>,
}

#[derive(Debug, Clone)]
pub struct TelopSegment {
    pub text: String,
    pub furigana: Option<String>,
    pub romaji: Option<String>,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone)]
pub struct CaptionTrack {
    pub language: String,
    pub label: String,
    pub segments: Vec<CaptionSegment>,
}

#[derive(Debug, Clone)]
pub struct CaptionSegment {
    pub text: String,
    pub start_ms: u64,
    pub duration_ms: u64,
}

/// Scoring / piano roll data.
#[derive(Debug, Clone)]
pub struct ScoringData {
    pub notes: Vec<ScoringNote>,
}

#[derive(Debug, Clone)]
pub struct ScoringNote {
    pub pitch: u8,
    pub start_ms: u64,
    pub duration_ms: u64,
}

/// How to actually play a song.
#[derive(Debug, Clone)]
pub enum MediaStream {
    /// HLS stream (DAM).
    Hls {
        url_high: String,
        url_low: Option<String>,
    },
    /// Direct video URL, possibly with separate audio (Joysound).
    DirectVideo {
        video_url: String,
        audio_url: Option<String>,
    },
    /// YouTube video to be downloaded via yt-dlp.
    YouTubeDownload { video_id: String },
}
