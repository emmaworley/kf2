use std::fmt;
use std::str::FromStr;
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

/// Stable URN identifying a piece of media. The wire format is
/// `urn:<provider_id>:<body>`, where `body` is provider-defined and may
/// contain further colon-separated qualifiers — DAM, for instance, prefixes
/// its content id with the field it came from so the URN reads
/// `urn:dam:contentsId:5778352`. If a provider ever switches keying schemes
/// the qualifier changes too, leaving old cache rows obviously distinguishable.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetId {
    provider: ProviderId,
    body: String,
}

impl AssetId {
    pub fn new(provider: ProviderId, body: impl Into<String>) -> Self {
        Self {
            provider,
            body: body.into(),
        }
    }

    pub fn provider(&self) -> ProviderId {
        self.provider
    }

    pub fn body(&self) -> &str {
        &self.body
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "urn:{}:{}", self.provider.as_str(), self.body)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AssetIdParseError {
    #[error("expected URN of form `urn:<provider>:<body>`, got `{0}`")]
    BadShape(String),
    #[error("unknown provider `{0}`")]
    UnknownProvider(String),
    #[error("empty body in URN `{0}`")]
    EmptyBody(String),
}

impl FromStr for AssetId {
    type Err = AssetIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // "urn:<provider>:<body>" — split off the first two segments and
        // keep everything past the second colon (including any further
        // colon-separated qualifiers) verbatim in `body`.
        let rest = s
            .strip_prefix("urn:")
            .ok_or_else(|| AssetIdParseError::BadShape(s.into()))?;
        let (provider_str, body) = rest
            .split_once(':')
            .ok_or_else(|| AssetIdParseError::BadShape(s.into()))?;
        if body.is_empty() {
            return Err(AssetIdParseError::EmptyBody(s.into()));
        }
        let provider = ProviderId::from_str(provider_str)
            .map_err(|_| AssetIdParseError::UnknownProvider(provider_str.into()))?;
        Ok(AssetId::new(provider, body))
    }
}

impl From<AssetId> for String {
    fn from(id: AssetId) -> String {
        id.to_string()
    }
}

impl TryFrom<String> for AssetId {
    type Error = AssetIdParseError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl serde::Serialize for AssetId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for AssetId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        raw.parse().map_err(serde::de::Error::custom)
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

/// What providers return as the "this is what we'll play" output: a stable
/// URN identity plus the remote location to fetch it from. The asset cache
/// transforms an `Asset` into a `CachedAsset` (in `crate::asset_cache`) by
/// downloading `source` to local storage; the `id` is what survives across
/// processes and gets stored in the `asset_cache` table.
#[derive(Debug, Clone)]
pub struct Asset {
    pub id: AssetId,
    pub source: MediaStream,
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
