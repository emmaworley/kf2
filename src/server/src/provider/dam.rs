use crate::provider::error::ProviderError;
use crate::provider::types::*;
use crate::provider::{ProviderSession, ScoringProvider, Searchable};
use backon::{ExponentialBuilder, Retryable};
use reqwest::{header, Url};
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

/// When set to a truthy value in the environment, `dam-tool` (and any other
/// in-process user of this module) will log every HTTP request and response
/// to stderr for debugging. Read once at first access; set the env var
/// before constructing a `DamProviderSession`.
static DAM_DEBUG: LazyLock<bool> = LazyLock::new(|| {
    matches!(
        std::env::var("DAM_DEBUG").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
});

fn dam_debug() -> bool {
    *DAM_DEBUG
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamConfig {
    pub username: String,
    pub password: String,
}

impl TryFrom<&ProviderConfig> for DamConfig {
    type Error = serde_json::Error;

    fn try_from(value: &ProviderConfig) -> Result<Self, Self::Error> {
        serde_json::from_value(value.0.clone())
    }
}

impl From<DamConfig> for ProviderConfig {
    fn from(cfg: DamConfig) -> Self {
        ProviderConfig(serde_json::to_value(cfg).expect("DamConfig is always serializable"))
    }
}

pub struct DamProvider;

impl DamProvider {
    pub const METADATA: ProviderMetadata = ProviderMetadata {
        id: ProviderId::Dam,
        name: "DAM",
        capabilities: &[Capability::Search, Capability::Scoring],
        requires_configuration: true,
    };

    pub fn client_builder() -> reqwest::ClientBuilder {
        let headers = header::HeaderMap::from_iter([(
            header::USER_AGENT,
            header::HeaderValue::from_static("WindowsApplication"),
        )]);
        reqwest::Client::builder()
            .cookie_store(true)
            .default_headers(headers)
    }

    /// Exchange credentials for a DAM session. The client must have been built
    /// via `client_builder()` (or an equivalent) so the DAM headers and cookie
    /// jar are in place.
    pub async fn login(
        client: &reqwest::Client,
        config: &DamConfig,
    ) -> Result<LoginPayload, ProviderError> {
        let url = &*URL_LOGIN;
        let fields = [
            ("loginId", config.username.as_str()),
            ("password", config.password.as_str()),
            ("format", "json"),
        ];
        log_request(url, &fields, &["password"]);
        let response = client
            .post(url.clone())
            .form(&fields)
            .header("win10-access-key", WIN10_ACCESS_KEY)
            .send()
            .await?;
        let body = fetch_body(url, response).await?;
        parse_minsei::<LoginPayload>(url, &body)
    }

    pub async fn configure(
        &self,
        config: Option<&ProviderConfig>,
    ) -> Result<Arc<dyn ProviderSession>, ProviderError> {
        let config = config
            .ok_or_else(|| ProviderError::AuthFailed("DAM requires username/password".into()))?;
        let cfg = DamConfig::try_from(config).map_err(|e| {
            ProviderError::AuthFailed(format!("DAM expects username/password config: {e}"))
        })?;
        let client = Self::client_builder()
            .build()
            .expect("Failed to build Reqwest client");
        let session = DamProviderSession::from_credentials(client, &cfg).await?;
        Ok(Arc::new(session))
    }
}

// ---------------------------------------------------------------------------
// Endpoint constants
// ---------------------------------------------------------------------------

const WIN10_ACCESS_KEY: &str = "mbAmgk3GuCOKAgL8dCQR";

static URL_LOGIN: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://win10.clubdam.com/cwa/win/minsei/auth/LoginByDamtomoMemberId.api").unwrap()
});
static URL_GET_STREAMING_URL: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://win10.clubdam.com/cwa/win/minsei/music/playLog/GetMusicStreamingURL.api")
        .unwrap()
});
static URL_GET_MUSIC_DETAIL: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://csgw.clubdam.com/dkwebsys/search-api/GetMusicDetailInfoApi").unwrap()
});
static URL_SEARCH_MUSIC: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://csgw.clubdam.com/dkwebsys/search-api/SearchMusicByKeywordApi").unwrap()
});
static URL_SEARCH_ARTIST: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://csgw.clubdam.com/dkwebsys/search-api/SearchArtistByKeywordApi").unwrap()
});
static URL_MUSIC_BY_ARTIST: LazyLock<Url> = LazyLock::new(|| {
    Url::parse("https://csgw.clubdam.com/dkwebsys/search-api/GetMusicListByArtistApi").unwrap()
});

const BASE_MINSEI_REQUEST: [(&str, &str); 7] = [
    ("charset", "UTF-8"),
    ("compAuthKey", "2/Qb9R@8s*"),
    ("compId", "1"),
    ("deviceId", "22"),
    ("format", "json"),
    ("serviceId", "1"),
    ("contractId", "1"),
];

const BASE_DKWEBSYS_REQUEST: [(&str, &str); 4] = [
    ("modelTypeCode", "2"),
    ("minseiModelNum", "M1"),
    ("compId", "1"),
    ("authKey", "2/Qb9R@8s*"),
];

// ---------------------------------------------------------------------------
// Envelopes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MinseiResponseBase {
    message: String,
    status: String,
    status_code: String,
}

/// Full Minsei envelope. `P` carries the endpoint-specific `data` / `list`
/// fields; the status/message/statusCode fields sit alongside them at top
/// level and are flattened in from `MinseiResponseBase`.
#[derive(Debug, Deserialize)]
struct MinseiEnvelope<P> {
    #[serde(flatten)]
    base: MinseiResponseBase,
    #[serde(flatten)]
    payload: P,
}

impl<P> MinseiEnvelope<P> {
    fn error_for_status(self) -> Result<Self, ProviderError> {
        // statusCode 1005 seems to mean pagination continues
        match self.base.status_code.as_str() {
            "0000" | "1005" => Ok(self),
            _ => Err(ProviderError::Upstream(format!(
                "{}: {}",
                self.base.status, self.base.message
            ))),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DkwebsysResult {
    status_code: String,
    message: String,
    #[serde(default)]
    detail_message: Option<String>,
}

/// DKWebsys envelope. Unlike Minsei, the status fields are nested under a
/// `result` key.
#[derive(Debug, Deserialize)]
struct DkwebsysEnvelope<P> {
    result: DkwebsysResult,
    #[serde(flatten)]
    payload: P,
}

impl<P> DkwebsysEnvelope<P> {
    fn error_for_status(self) -> Result<Self, ProviderError> {
        if self.result.status_code == "0000" {
            Ok(self)
        } else {
            Err(ProviderError::Upstream(format!(
                "{} {}: {}",
                self.result.status_code,
                self.result.message,
                self.result.detail_message.as_deref().unwrap_or("")
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Payload types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginPayload {
    pub data: LoginData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginData {
    pub auth_token: String,
    pub damtomo_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamingUrlsPayload {
    list: Vec<StreamingUrlsListItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamingUrlsListItem {
    duet: String,
    high_bitrate_url: String,
    low_bitrate_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetMusicDetailInfoPayload {
    data: GetMusicDetailInfoData,
    list: Vec<GetMusicDetailInfoListGroup>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetMusicDetailInfoData {
    request_no: String,
    title: String,
    artist: String,
}

#[derive(Debug, Deserialize)]
struct GetMusicDetailInfoListGroup {
    #[serde(rename = "mModelMusicInfoList")]
    items: Vec<GetMusicDetailInfoItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetMusicDetailInfoItem {
    playtime: String,
    guide_vocal: String,
    score_flag: String,
    score_level: u32,
    technical_level: u32,
    shift: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DamPaginationMeta {
    total_count: u32,
    has_next: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = "I: Deserialize<'de>"))]
struct PaginatedPayload<I> {
    data: DamPaginationMeta,
    list: Vec<I>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchMusicListItem {
    request_no: String,
    title: String,
    artist: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchArtistListItem {
    artist: String,
    artist_code: i64,
}

// ---------------------------------------------------------------------------
// Conversions (upstream shape → domain types)
// ---------------------------------------------------------------------------

impl From<GetMusicDetailInfoPayload> for Song {
    fn from(p: GetMusicDetailInfoPayload) -> Self {
        let first_item = p.list.into_iter().flat_map(|g| g.items.into_iter()).next();

        let (duration, extra) = match first_item {
            Some(item) => {
                let duration = item.playtime.parse::<u64>().ok().map(Duration::from_secs);
                let vocal_types = match item.guide_vocal.as_str() {
                    "1" => vec![DamVocalType::GuideMale],
                    "2" => vec![DamVocalType::GuideFemale],
                    _ => vec![DamVocalType::Normal],
                };
                let extra = DamSongExtra {
                    vocal_types,
                    has_scoring: item.score_flag == "1",
                    score_level: item.score_level,
                    technical_level: item.technical_level,
                    shift: parse_shift(item.shift.as_str()),
                };
                (duration, extra)
            }
            None => (None, DamSongExtra::default()),
        };

        Song {
            provider: ProviderId::Dam,
            id: p.data.request_no,
            title: p.data.title,
            artist: p.data.artist,
            duration,
            extra: SongExtra::Dam(extra),
        }
    }
}

impl<I, T> From<PaginatedPayload<I>> for SearchResults<T>
where
    T: From<I>,
{
    fn from(p: PaginatedPayload<I>) -> Self {
        SearchResults {
            items: p.list.into_iter().map(T::from).collect(),
            total_count: p.data.total_count,
            has_more: p.data.has_next == "1",
        }
    }
}

impl From<SearchMusicListItem> for SongResult {
    fn from(item: SearchMusicListItem) -> Self {
        SongResult {
            provider: ProviderId::Dam,
            id: item.request_no,
            title: item.title,
            artist: item.artist,
        }
    }
}

impl From<SearchArtistListItem> for Artist {
    fn from(item: SearchArtistListItem) -> Self {
        Artist {
            provider: ProviderId::Dam,
            id: item.artist_code.to_string(),
            name: item.artist,
        }
    }
}

impl TryFrom<StreamingUrlsPayload> for MediaStream {
    type Error = ProviderError;

    fn try_from(p: StreamingUrlsPayload) -> Result<Self, Self::Error> {
        let non_duet_idx = p.list.iter().position(|e| e.duet != "1");
        let picked = match non_duet_idx {
            Some(i) => p.list.into_iter().nth(i).unwrap(),
            None => p
                .list
                .into_iter()
                .next()
                .ok_or_else(|| ProviderError::NotFound("no streaming urls returned".into()))?,
        };
        Ok(MediaStream::Hls {
            url_high: picked.high_bitrate_url,
            url_low: Some(picked.low_bitrate_url),
        })
    }
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

pub struct DamProviderSession {
    client: reqwest::Client,
    user_code: String,
    auth_token: String,
}

impl DamProviderSession {
    pub fn from_tokens(client: reqwest::Client, user_code: String, auth_token: String) -> Self {
        Self {
            client,
            user_code,
            auth_token,
        }
    }

    pub async fn from_credentials(
        client: reqwest::Client,
        config: &DamConfig,
    ) -> Result<Self, ProviderError> {
        let login = DamProvider::login(&client, config).await?;
        Ok(Self::from_tokens(
            client,
            config.username.clone(),
            login.data.auth_token,
        ))
    }

    async fn post_minsei<P: DeserializeOwned>(
        &self,
        url: &Url,
        body: &[(&str, &str)],
    ) -> Result<P, ProviderError> {
        let fields: Vec<(&str, &str)> = BASE_MINSEI_REQUEST
            .iter()
            .copied()
            .chain(body.iter().copied())
            .collect();
        log_request(url, &fields, &["authToken"]);
        let response = self
            .client
            .post(url.clone())
            .form(&fields)
            .header("win10-access-key", WIN10_ACCESS_KEY)
            .send()
            .await?;
        let body = fetch_body(url, response).await?;
        parse_minsei::<P>(url, &body)
    }

    async fn post_dkwebsys<P: DeserializeOwned>(
        &self,
        url: &Url,
        body: &[(&str, &str)],
    ) -> Result<P, ProviderError> {
        let fields: Vec<(&str, &str)> = BASE_DKWEBSYS_REQUEST
            .iter()
            .copied()
            .chain(body.iter().copied())
            .collect();
        log_request(url, &fields, &[]);
        let payload: std::collections::HashMap<&str, &str> = fields.iter().copied().collect();
        debug_assert_eq!(
            payload.len(),
            fields.len(),
            "duplicate keys in dkwebsys request"
        );
        let response = self.client.post(url.clone()).json(&payload).send().await?;
        let body = fetch_body(url, response).await?;
        parse_dkwebsys::<P>(url, &body)
    }
}

/// Read an HTTP response, logging the body + status when the request failed.
/// On success, returns the raw body text for envelope-aware decoding.
async fn fetch_body(url: &Url, response: reqwest::Response) -> Result<String, ProviderError> {
    let status = response.status();
    let body = response.text().await?;
    if dam_debug() {
        eprintln!("[dam] <- {status} {url}\n{body}");
    }
    if !status.is_success() {
        let snippet = truncate(&body, 1024);
        if !dam_debug() {
            eprintln!("[dam] HTTP {status} from {url}\n  body: {snippet}");
        }
        return Err(ProviderError::Upstream(format!(
            "HTTP {status} from {url}: {snippet}"
        )));
    }
    Ok(body)
}

/// Stderr-log an outgoing request when `DAM_DEBUG` is set. `fields` lists
/// every `(key, value)` pair that will be sent; `redact` holds keys whose
/// values should be replaced with `***` in the log (e.g. passwords).
fn log_request(url: &Url, fields: &[(&str, &str)], redact: &[&str]) {
    if !dam_debug() {
        return;
    }
    eprint!("[dam] -> POST {url}");
    for (k, v) in fields {
        let shown = if redact.contains(k) { "***" } else { *v };
        eprint!(" {k}={shown}");
    }
    eprintln!();
}

/// Two-stage decode for Minsei responses: first parse only the status base so
/// upstream-signalled errors surface as `ProviderError::Upstream` instead of a
/// misleading serde "missing field" error, then parse the typed payload on
/// success.
fn parse_minsei<P: DeserializeOwned>(url: &Url, body: &str) -> Result<P, ProviderError> {
    let status: MinseiEnvelope<IgnoredAny> =
        serde_json::from_str(body).map_err(|e| json_decode_error(url, body, e))?;
    status.error_for_status()?;
    let envelope: MinseiEnvelope<P> =
        serde_json::from_str(body).map_err(|e| json_decode_error(url, body, e))?;
    Ok(envelope.payload)
}

/// Two-stage decode for DKWebsys responses; see [`parse_minsei`].
fn parse_dkwebsys<P: DeserializeOwned>(url: &Url, body: &str) -> Result<P, ProviderError> {
    let status: DkwebsysEnvelope<IgnoredAny> =
        serde_json::from_str(body).map_err(|e| json_decode_error(url, body, e))?;
    status.error_for_status()?;
    let envelope: DkwebsysEnvelope<P> =
        serde_json::from_str(body).map_err(|e| json_decode_error(url, body, e))?;
    Ok(envelope.payload)
}

fn json_decode_error(url: &Url, body: &str, err: serde_json::Error) -> ProviderError {
    let snippet = truncate(body, 1024);
    eprintln!("[dam] JSON decode error from {url}: {err}\n  body: {snippet}");
    ProviderError::Upstream(format!("JSON decode error from {url}: {err}"))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let cut = s.floor_char_boundary(max);
        format!("{}… [truncated {} bytes]", &s[..cut], s.len() - cut)
    }
}

fn page_no_string(page: u32) -> String {
    (page + 1).to_string()
}

fn parse_shift(s: &str) -> i32 {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0;
    }
    trimmed
        .strip_prefix('+')
        .unwrap_or(trimmed)
        .parse()
        .ok()
        .unwrap_or(0)
}

fn is_retryable(e: &ProviderError) -> bool {
    matches!(e, ProviderError::Http(_))
}

fn minsei_backoff() -> ExponentialBuilder {
    ExponentialBuilder::default()
        .with_max_times(3)
        .with_min_delay(Duration::from_millis(200))
}

#[tonic::async_trait]
impl ProviderSession for DamProviderSession {
    async fn get_song(&self, song_id: &str) -> Result<Song, ProviderError> {
        let payload: GetMusicDetailInfoPayload = self
            .post_dkwebsys(&URL_GET_MUSIC_DETAIL, &[("requestNo", song_id)])
            .await?;
        Ok(payload.into())
    }

    async fn get_stream(&self, song_id: &str) -> Result<MediaStream, ProviderError> {
        let payload: StreamingUrlsPayload = (|| async {
            self.post_minsei(
                &URL_GET_STREAMING_URL,
                &[
                    ("requestNo", song_id),
                    ("userCode", self.user_code.as_str()),
                    ("authToken", self.auth_token.as_str()),
                ],
            )
            .await
        })
        .retry(minsei_backoff())
        .when(is_retryable)
        .await?;
        MediaStream::try_from(payload)
    }

    fn as_searchable(&self) -> Option<&dyn Searchable> {
        Some(self)
    }

    fn as_scoring_provider(&self) -> Option<&dyn ScoringProvider> {
        Some(self)
    }
}

#[tonic::async_trait]
impl Searchable for DamProviderSession {
    async fn search_songs(
        &self,
        query: &str,
        page: u32,
    ) -> Result<SearchResults<SongResult>, ProviderError> {
        let page_no = page_no_string(page);
        let payload: PaginatedPayload<SearchMusicListItem> = self
            .post_dkwebsys(
                &URL_SEARCH_MUSIC,
                &[
                    ("keyword", query),
                    ("sort", "2"),
                    ("pageNo", page_no.as_str()),
                    ("dispCount", "30"),
                ],
            )
            .await?;
        Ok(payload.into())
    }

    async fn search_artists(
        &self,
        query: &str,
        page: u32,
    ) -> Result<SearchResults<Artist>, ProviderError> {
        let page_no = page_no_string(page);
        let payload: PaginatedPayload<SearchArtistListItem> = self
            .post_dkwebsys(
                &URL_SEARCH_ARTIST,
                &[
                    ("keyword", query),
                    ("sort", "2"),
                    ("pageNo", page_no.as_str()),
                    ("dispCount", "30"),
                ],
            )
            .await?;
        Ok(payload.into())
    }

    async fn songs_by_artist(
        &self,
        artist_id: &str,
        page: u32,
    ) -> Result<SearchResults<SongResult>, ProviderError> {
        let page_no = page_no_string(page);
        let payload: PaginatedPayload<SearchMusicListItem> = self
            .post_dkwebsys(
                &URL_MUSIC_BY_ARTIST,
                &[
                    ("artistCode", artist_id),
                    ("sort", "2"),
                    ("pageNo", page_no.as_str()),
                    ("dispCount", "30"),
                ],
            )
            .await?;
        Ok(payload.into())
    }
}

#[tonic::async_trait]
impl ScoringProvider for DamProviderSession {
    async fn get_scoring(&self, _song_id: &str) -> Result<ScoringData, ProviderError> {
        // Upstream returns an opaque application/octet-stream blob whose
        // format isn't yet parsed here. Deferred until we have a concrete
        // consumer that needs structured notes.
        Err(ProviderError::NotSupported)
    }
}
