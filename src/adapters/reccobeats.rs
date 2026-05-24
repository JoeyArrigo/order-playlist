// pattern: Imperative Shell — file IO / HTTP / cache persistence

//! ReccoBeats audio-features adapter. Gated by the `reccobeats` Cargo feature.
//!
//! Resolves track IDs (ISRCs) to their audio features via the ReccoBeats API.
//!
//! Single surface exposed: `FeatureSource::features_for(&[TrackId]) -> Vec<(TrackId, Option<TrackFeatures>)>`.
//! Cache read-through filters un-cached IDs before any network call.
//!
//! ## API shape (verified via Task 1 spike)
//!
//! - Endpoint: `GET /v1/audio-features?ids=<id1>&ids=<id2>...` (batch query-string, ISRC identifiers)
//! - Response shape: `{ "content": [FeatureItem...] }` (note: `content` not `items`)
//! - Each item includes: `href` (Spotify URL), `isrc`, `tempo`, `key` (0-11), `mode` (0=minor, 1=major),
//!   `energy`, `danceability`, `valence`, `loudness`, `acousticness`, `instrumentalness`, `liveness`, `speechiness`.
//! - No-match: API omits unmatched entries from the response.
//!
//! ## Camelot cost degradation
//!
//! Both `key` and `mode` are present in the actual API (contrary to design research).
//! No fallback to uniform costs needed — the full Camelot distance term is usable.

use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::adapters::{Cache, FeatureSource};
use crate::domain::{Bpm, Mode, Normalized, PitchClass, TrackFeatures, TrackId};
use crate::errors::{ReccoBeatsError, ReccoBeatsErrorKind};

/// API base URL.
const DEFAULT_BASE: &str = "https://api.reccobeats.com/v1";

/// Maximum IDs per batch (spike-verified as safe).
const MAX_BATCH: usize = 40;

/// Response wrapper from ReccoBeats API.
#[derive(serde::Deserialize, Debug)]
struct BatchResponse {
    /// Array of feature items (API uses "content", not "items").
    #[serde(default)]
    content: Vec<FeatureItem>,
}

/// A single track's audio features from ReccoBeats.
#[derive(serde::Deserialize, Debug, Default)]
pub(crate) struct FeatureItem {
    /// Spotify track URL (used as fallback for ID mapping).
    #[serde(default)]
    href: Option<String>,

    /// ISRC identifier (primary mapping key).
    #[serde(default)]
    isrc: Option<String>,

    /// Numeric audio features (all optional per serde::default).
    #[serde(default)]
    tempo: Option<f32>,

    /// Pitch class 0-11.
    #[serde(default)]
    key: Option<u8>,

    /// Mode: 0 = minor, 1 = major (Spotify convention).
    #[serde(default)]
    mode: Option<u8>,

    /// Energy [0.0, 1.0].
    #[serde(default)]
    energy: Option<f32>,

    /// Danceability [0.0, 1.0].
    #[serde(default)]
    danceability: Option<f32>,

    /// Valence (positivity) [0.0, 1.0].
    #[serde(default)]
    valence: Option<f32>,

    /// Loudness in dB.
    #[serde(default)]
    loudness: Option<f32>,

    /// Acousticness [0.0, 1.0].
    #[serde(default)]
    acousticness: Option<f32>,

    /// Instrumentalness [0.0, 1.0].
    #[serde(default)]
    instrumentalness: Option<f32>,

    /// Liveness [0.0, 1.0].
    #[serde(default)]
    liveness: Option<f32>,

    /// Speechiness [0.0, 1.0].
    #[serde(default)]
    speechiness: Option<f32>,
}

/// ReccoBeats feature source with batching, caching, and 429 retry.
pub struct ReccoBeatsFeatures {
    client: Client,
    cache: Arc<Mutex<Cache>>,
    base: String,
    /// Number of 429-retry attempts per batch. Default 1 (one retry).
    pub retry_attempts: u8,
    /// Base duration for exponential backoff on 429 errors.
    /// Default `Duration::from_secs(2)` for production; tests override to `Duration::from_millis(10)`.
    pub retry_backoff_base: Duration,
}

impl ReccoBeatsFeatures {
    /// Construct a feature source with default base URL and production settings.
    pub fn new(cache: Arc<Mutex<Cache>>) -> Result<Self, reqwest::Error> {
        Self::new_with_base(cache, DEFAULT_BASE.to_string())
    }

    /// Construct a feature source with a custom base URL (for testing).
    pub fn new_with_base(cache: Arc<Mutex<Cache>>, base: String) -> Result<Self, reqwest::Error> {
        let client = Client::builder().timeout(Duration::from_secs(30)).build()?;
        Ok(Self {
            client,
            cache,
            base,
            retry_attempts: 1,
            retry_backoff_base: Duration::from_secs(2),
        })
    }
}

/// Build a batch URL with multiple IDs as query parameters.
///
/// Each ID is URL-encoded and joined with `&`.
pub(crate) fn build_batch_url(base: &str, ids: &[&str]) -> String {
    let qs: String = ids
        .iter()
        .map(|id| format!("ids={}", urlencoding::encode(id)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}/audio-features?{qs}")
}

/// Convert a parsed `FeatureItem` to a `TrackFeatures` with sensible
/// fallbacks for missing fields.
pub(crate) fn item_to_features(item: &FeatureItem) -> TrackFeatures {
    let tempo = item
        .tempo
        .and_then(|t| Bpm::new(t).ok())
        .unwrap_or(Bpm::DEFAULT_120);

    let key = item
        .key
        .and_then(|k| PitchClass::new(k).ok())
        .unwrap_or(PitchClass::C);

    let mode = match item.mode {
        Some(0) => Mode::Minor,
        Some(_) => Mode::Major,
        None => Mode::Major,
    };

    TrackFeatures {
        tempo,
        key,
        mode,
        energy: item
            .energy
            .map(Normalized::clamp)
            .unwrap_or(Normalized::HALF),
        danceability: item
            .danceability
            .map(Normalized::clamp)
            .unwrap_or(Normalized::HALF),
        valence: item
            .valence
            .map(Normalized::clamp)
            .unwrap_or(Normalized::HALF),
        loudness: item.loudness.unwrap_or(-10.0),
        acousticness: item
            .acousticness
            .map(Normalized::clamp)
            .unwrap_or(Normalized::HALF),
        instrumentalness: item
            .instrumentalness
            .map(Normalized::clamp)
            .unwrap_or(Normalized::ZERO),
        liveness: item
            .liveness
            .map(Normalized::clamp)
            .unwrap_or(Normalized::ZERO),
        speechiness: item
            .speechiness
            .map(Normalized::clamp)
            .unwrap_or(Normalized::ZERO),
    }
}

/// Map a `FeatureItem` back to the input `TrackId` that produced it.
///
/// Tries `isrc` first (case-insensitive), then `href` (extract Spotify ID from URL).
pub(crate) fn match_item_to_id<'a>(item: &FeatureItem, ids: &'a [TrackId]) -> Option<&'a TrackId> {
    // Primary: match by ISRC (case-insensitive).
    if let Some(isrc) = &item.isrc {
        if let Some(id) = ids.iter().find(|i| i.get().eq_ignore_ascii_case(isrc)) {
            return Some(id);
        }
    }

    // Fallback: extract Spotify ID from href ("https://open.spotify.com/track/<id>").
    if let Some(href) = &item.href {
        if let Some(id_str) = href.rsplit('/').next() {
            if let Some(id) = ids.iter().find(|i| i.get() == id_str) {
                return Some(id);
            }
        }
    }

    None
}

#[async_trait::async_trait]
impl FeatureSource for ReccoBeatsFeatures {
    async fn features_for(&self, ids: &[TrackId]) -> Vec<(TrackId, Option<TrackFeatures>)> {
        let mut output: Vec<(TrackId, Option<TrackFeatures>)> = Vec::with_capacity(ids.len());
        let mut to_fetch: Vec<TrackId> = Vec::new();

        // Cache read-through.
        {
            let cache = self.cache.lock().await;
            for id in ids {
                if let Some(features) = cache.get_features(id) {
                    output.push((id.clone(), Some(features.clone())));
                } else {
                    output.push((id.clone(), None));
                    to_fetch.push(id.clone());
                }
            }
        }

        if to_fetch.is_empty() {
            return output;
        }

        // Fetch in batches.
        for batch in to_fetch.chunks(MAX_BATCH) {
            let id_strs: Vec<&str> = batch.iter().map(|i| i.get()).collect();
            let url = build_batch_url(&self.base, &id_strs);

            tracing::info!(batch_size = id_strs.len(), url = %url, "reccobeats batch");

            match self.fetch_with_retry(&url, &id_strs).await {
                Ok(items) => {
                    let mut cache = self.cache.lock().await;
                    for item in &items {
                        if let Some(id) = match_item_to_id(item, batch) {
                            let features = item_to_features(item);
                            cache.put_features(id.clone(), features.clone());
                            // Backfill the output slot for this id.
                            if let Some(slot) = output.iter_mut().find(|(o_id, _)| o_id == id) {
                                slot.1 = Some(features);
                            }
                        } else {
                            tracing::warn!(
                                item_isrc = ?item.isrc,
                                item_href = ?item.href,
                                batch_size = batch.len(),
                                "reccobeats response item didn't match any input id"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error_kind = ?e.kind,
                        ids = ?e.ids,
                        "reccobeats batch failed; leaving as None"
                    );
                    // Do NOT cache failure. Output slots stay None;
                    // Phase 7 orchestration will emit them as unresolved.
                }
            }
        }

        output
    }
}

impl ReccoBeatsFeatures {
    /// Fetch with retry logic for HTTP 429 errors.
    async fn fetch_with_retry(
        &self,
        url: &str,
        ids: &[&str],
    ) -> Result<Vec<FeatureItem>, ReccoBeatsError> {
        for attempt in 0..=self.retry_attempts {
            match self.fetch_once(url).await {
                Ok(items) => return Ok(items),
                Err(e)
                    if matches!(e.kind, ReccoBeatsErrorKind::Throttled)
                        && attempt < self.retry_attempts =>
                {
                    // Exponential backoff: base * 2^(attempt+1).
                    let backoff = self
                        .retry_backoff_base
                        .mul_f32(2f32.powi(attempt as i32 + 1));
                    tracing::info!(
                        attempt = attempt + 1,
                        backoff_ms = backoff.as_millis(),
                        "reccobeats throttled; retrying"
                    );
                    tokio::time::sleep(backoff).await;
                }
                Err(mut e) => {
                    e.ids = ids.iter().map(|s| s.to_string()).collect();
                    return Err(e);
                }
            }
        }

        Err(ReccoBeatsError {
            kind: ReccoBeatsErrorKind::Throttled,
            ids: ids.iter().map(|s| s.to_string()).collect(),
            source: None,
        })
    }

    /// Fetch features once without retry.
    async fn fetch_once(&self, url: &str) -> Result<Vec<FeatureItem>, ReccoBeatsError> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| ReccoBeatsError {
                kind: ReccoBeatsErrorKind::Network,
                ids: Vec::new(),
                source: Some(e),
            })?;

        if resp.status().as_u16() == 429 {
            return Err(ReccoBeatsError {
                kind: ReccoBeatsErrorKind::Throttled,
                ids: Vec::new(),
                source: None,
            });
        }

        let resp = resp.error_for_status().map_err(|e| ReccoBeatsError {
            kind: ReccoBeatsErrorKind::Network,
            ids: Vec::new(),
            source: Some(e),
        })?;

        let body: BatchResponse = resp.json().await.map_err(|e| ReccoBeatsError {
            kind: ReccoBeatsErrorKind::Parse,
            ids: Vec::new(),
            source: Some(e),
        })?;

        Ok(body.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_batch_url_joins_ids_with_ampersand() {
        let url = build_batch_url(
            "https://api.reccobeats.com/v1",
            &["AAA111111111", "BBB222222222"],
        );
        assert!(
            url.contains("ids=AAA111111111"),
            "first ID should be in URL"
        );
        assert!(
            url.contains("ids=BBB222222222"),
            "second ID should be in URL"
        );
        assert!(url.contains("&"), "IDs should be joined with ampersand");
        assert!(
            url.starts_with("https://api.reccobeats.com/v1/audio-features?"),
            "URL should have correct base"
        );
    }

    #[test]
    fn build_batch_url_url_encodes_ids() {
        let url = build_batch_url("https://api.reccobeats.com/v1", &["ID with spaces"]);
        assert!(
            url.contains("ID%20with%20spaces"),
            "spaces should be URL-encoded"
        );
    }

    #[test]
    fn item_to_features_uses_defaults_when_fields_missing() {
        let item = FeatureItem::default();
        let f = item_to_features(&item);
        assert_eq!(f.tempo.get(), 120.0, "tempo should default to 120");
        assert_eq!(f.key.get(), 0, "key should default to C (0)");
        assert!(
            matches!(f.mode, Mode::Major),
            "mode should default to Major"
        );
        assert_eq!(
            f.energy.get(),
            0.5,
            "energy should default to HALF when missing"
        );
        assert_eq!(
            f.instrumentalness.get(),
            0.0,
            "instrumentalness should default to ZERO when missing"
        );
    }

    #[test]
    fn item_to_features_uses_provided_values() {
        let item = FeatureItem {
            tempo: Some(116.0),
            key: Some(9),
            mode: Some(1),
            energy: Some(0.8),
            danceability: Some(0.7),
            valence: Some(0.6),
            loudness: Some(-6.0),
            acousticness: Some(0.13),
            instrumentalness: Some(0.001),
            liveness: Some(0.127),
            speechiness: Some(0.027),
            ..Default::default()
        };
        let f = item_to_features(&item);
        assert_eq!(f.tempo.get(), 116.0);
        assert_eq!(f.key.get(), 9);
        assert!(matches!(f.mode, Mode::Major));
        assert_eq!(f.energy.get(), 0.8);
        assert_eq!(f.loudness, -6.0);
    }

    #[test]
    fn match_item_to_id_by_isrc_case_insensitive() {
        let ids = vec![TrackId::new("USQX91300120"), TrackId::new("USQX91100008")];
        let item = FeatureItem {
            isrc: Some("usqx91300120".into()), // lowercase
            ..Default::default()
        };
        assert_eq!(
            match_item_to_id(&item, &ids),
            Some(&ids[0]),
            "should match by ISRC case-insensitively"
        );
    }

    #[test]
    fn match_item_to_id_by_href_spotify_url() {
        let ids = vec![TrackId::new("69kOkLUCkxIZYexIgSG8rq")];
        let item = FeatureItem {
            href: Some("https://open.spotify.com/track/69kOkLUCkxIZYexIgSG8rq".into()),
            isrc: None,
            ..Default::default()
        };
        assert_eq!(
            match_item_to_id(&item, &ids),
            Some(&ids[0]),
            "should extract Spotify ID from href and match"
        );
    }

    #[test]
    fn match_item_to_id_prefers_isrc_over_href() {
        let ids = vec![
            TrackId::new("USQX91300120"),
            TrackId::new("69kOkLUCkxIZYexIgSG8rq"),
        ];
        let item = FeatureItem {
            isrc: Some("usqx91300120".into()),
            href: Some("https://open.spotify.com/track/69kOkLUCkxIZYexIgSG8rq".into()),
            ..Default::default()
        };
        assert_eq!(
            match_item_to_id(&item, &ids),
            Some(&ids[0]),
            "should match by ISRC first, not href"
        );
    }

    #[test]
    fn match_item_to_id_returns_none_when_no_match() {
        let ids = vec![TrackId::new("UNMATCHED")];
        let item = FeatureItem {
            isrc: Some("DIFFERENT".into()),
            href: Some("https://open.spotify.com/track/other_id".into()),
            ..Default::default()
        };
        assert_eq!(match_item_to_id(&item, &ids), None);
    }

    #[test]
    fn feature_item_default_derives() {
        let item = FeatureItem::default();
        assert!(item.href.is_none());
        assert!(item.isrc.is_none());
        assert!(item.tempo.is_none());
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use wiremock::{matchers::method, matchers::path, Mock, MockServer, ResponseTemplate};

    async fn make_features(mock: &MockServer) -> ReccoBeatsFeatures {
        let cache = Arc::new(Mutex::new(
            Cache::load(std::path::Path::new("/nonexistent")).unwrap(),
        ));
        let mut src =
            ReccoBeatsFeatures::new_with_base(cache, format!("{}/v1", mock.uri())).unwrap();
        // Set fast backoff for tests.
        src.retry_backoff_base = Duration::from_millis(10);
        src
    }

    #[tokio::test]
    async fn happy_path_batch_of_two() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/audio-features"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [
                    { "isrc": "USQX91300120", "tempo": 116.0, "energy": 0.8, "danceability": 0.7, "valence": 0.6, "loudness": -6.0 },
                    { "isrc": "USQX91100008", "tempo": 120.0, "energy": 0.5, "danceability": 0.5, "valence": 0.5, "loudness": -10.0 }
                ]
            })))
            .mount(&mock)
            .await;

        let src = make_features(&mock).await;
        let ids = vec![TrackId::new("USQX91300120"), TrackId::new("USQX91100008")];
        let result = src.features_for(&ids).await;
        assert_eq!(result.len(), 2, "should return two results");
        assert!(
            result.iter().all(|(_, f)| f.is_some()),
            "both should have features"
        );
        assert_eq!(
            result[0].1.as_ref().unwrap().tempo.get(),
            116.0,
            "first should have correct tempo"
        );
    }

    #[tokio::test]
    async fn cache_hit_bypasses_network() {
        let mock = MockServer::start().await;
        // Mount NO mocks — any HTTP call to `mock` fails the test.

        let cache = Arc::new(Mutex::new(
            Cache::load(std::path::Path::new("/nonexistent")).unwrap(),
        ));
        {
            let mut c = cache.lock().await;
            c.put_features(TrackId::new("USQX91300120"), TrackFeatures::neutral());
        }
        let src = ReccoBeatsFeatures::new_with_base(cache, format!("{}/v1", mock.uri())).unwrap();
        let ids = vec![TrackId::new("USQX91300120")];
        let result = src.features_for(&ids).await;
        assert!(result[0].1.is_some(), "cached ID should return Some");
    }

    #[tokio::test]
    async fn http_429_then_200_succeeds() {
        let mock = MockServer::start().await;

        // First call: 429.
        Mock::given(method("GET"))
            .and(path("/v1/audio-features"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&mock)
            .await;

        // Second call: 200 with features.
        Mock::given(method("GET"))
            .and(path("/v1/audio-features"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{ "isrc": "USQX91300120", "tempo": 116.0 }]
            })))
            .mount(&mock)
            .await;

        let mut src = make_features(&mock).await;
        src.retry_attempts = 1;
        let ids = vec![TrackId::new("USQX91300120")];
        let result = src.features_for(&ids).await;
        assert!(result[0].1.is_some(), "expected feature after retry on 429");
    }

    #[tokio::test]
    async fn unmatchable_id_remains_none() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/audio-features"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": []
            })))
            .mount(&mock)
            .await;

        let src = make_features(&mock).await;
        let ids = vec![TrackId::new("ZZZZ99999999")];
        let result = src.features_for(&ids).await;
        assert!(result[0].1.is_none(), "unmatched ID should remain None");
    }
}
