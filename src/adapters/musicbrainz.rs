// pattern: Imperative Shell — file IO / HTTP / cache persistence

//! MusicBrainz WS2 resolver. Gated by the `musicbrainz` Cargo feature.
//!
//! For each TrackQuery:
//!   1. Search `/ws/2/recording?query=title:X AND artist:Y&fmt=json&limit=10`.
//!   2. Filter out recordings whose `disambiguation` matches
//!      `live|remix|demo|instrumental|acoustic|karaoke|cover`
//!      (case-insensitive substring match).
//!   3. For each remaining candidate (highest `score` first), call
//!      `/ws/2/recording/{mbid}?inc=isrcs&fmt=json`. Return the first
//!      non-empty `isrcs[0]` as `TrackId(isrc)`.
//!   4. If no candidate yields an ISRC, return `Resolution::Unresolved`
//!      with a human-readable reason.
//!
//! Pacing: ≥1100 ms between requests via `tokio::time::Interval`.
//!
//! **Cache and explicit-failure sentinel:** The `Cache` persists both successes
//! and explicit failures (unresolvable queries). An explicit failure is represented
//! as a `TrackId("")` — an empty string that differentiates "we tried and found nothing"
//! from "not yet attempted". This keeps the cache shape uniform and avoids Phase 4
//! retrofits to wrap `TrackId` in `Option`.

use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Interval;

use crate::adapters::{Cache, Resolution, Resolver};
use crate::domain::{TrackId, TrackQuery};
use crate::errors::{MusicBrainzError, MusicBrainzErrorKind};

/// Default MusicBrainz WS2 base URL. Overridable per-instance via
/// `MusicBrainzIsrcResolver::new_with_base` (for testing with wiremock)
/// so tests can point at a mock server. URL builders take the base as a `&str`
/// argument — keeping them functions (not methods) keeps unit tests trivial
/// and avoids needing a resolver instance to test URL construction.
pub const DEFAULT_BASE: &str = "https://musicbrainz.org/ws/2";

/// Lucene special characters that must be backslash-escaped.
///
/// Note: Single `&` and `|` are NOT escaped because they are not special in Lucene
/// (only the paired operators `&&` and `||` are). Real-world music titles rarely
/// contain `&&` or `||` as Lucene operators, so we don't add pair-detection logic.
const LUCENE_SPECIALS: &[char] = &[
    '+', '-', '!', '(', ')', '{', '}', '[', ']', '^', '"', '~', '*', '?', ':', '\\', '/',
];

/// Escape Lucene special characters in a query string by prepending backslashes.
///
/// The Lucene query syntax reserves these characters: `+ - ! ( ) { } [ ] ^ " ~ * ? : \ /`.
/// This function escapes each of them individually to ensure they are treated literally
/// in the query. Note: `&&` and `||` are paired operators in Lucene, but single `&`
/// and `|` are not special and are not escaped.
pub(crate) fn escape_lucene(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        if LUCENE_SPECIALS.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Build a MusicBrainz search URL for recordings by title and artist.
///
/// Constructs a Lucene query `title:"<title>" AND artist:"<artist>"`,
/// escapes special characters, URL-encodes the query, and appends format
/// and limit parameters.
pub(crate) fn build_search_url(base: &str, title: &str, artist: &str, limit: u16) -> String {
    let q = format!(
        "title:\"{}\" AND artist:\"{}\"",
        escape_lucene(title),
        escape_lucene(artist)
    );
    format!(
        "{base}/recording?query={}&fmt=json&limit={}",
        urlencoding::encode(&q),
        limit
    )
}

/// Build a MusicBrainz lookup URL for a recording by MBID.
///
/// Includes the `inc=isrcs` parameter to request ISRC data in the response.
pub(crate) fn build_lookup_url(base: &str, mbid: &str) -> String {
    format!("{base}/recording/{mbid}?inc=isrcs&fmt=json")
}

/// Disambiguation phrases that mark a recording as non-studio for v1.
///
/// Recordings with these phrases in their `disambiguation` field are filtered
/// out during the search phase to prefer official studio releases.
pub(crate) const NON_STUDIO_MARKERS: &[&str] = &[
    "live",
    "remix",
    "demo",
    "instrumental",
    "acoustic",
    "karaoke",
    "cover",
];

/// Check if a recording is a studio recording based on its disambiguation field.
///
/// Returns `true` if the `disambiguation` field does NOT contain any of the
/// `NON_STUDIO_MARKERS` (case-insensitive substring match). Returns `true`
/// for empty disambiguation (which indicates a standard studio release).
pub(crate) fn is_studio(disambiguation: &str) -> bool {
    let lower = disambiguation.to_lowercase();
    !NON_STUDIO_MARKERS.iter().any(|m| lower.contains(m))
}

/// MusicBrainz search response containing a list of recordings.
#[derive(Deserialize, Debug)]
struct SearchResponse {
    /// Array of recordings matching the search query.
    recordings: Vec<SearchRecording>,
}

/// A recording returned by MusicBrainz search.
#[derive(Deserialize, Debug)]
struct SearchRecording {
    /// MBID (UUID) of the recording.
    id: String,
    /// Search score (higher = more relevant). Used for sorting candidates.
    score: i32,
    /// Optional disambiguation field. Defaults to empty string if missing.
    #[serde(default)]
    disambiguation: String,
}

/// MusicBrainz lookup response containing ISRC information.
#[derive(Deserialize, Debug)]
struct LookupResponse {
    /// Array of ISRCs for this recording. May be empty.
    #[serde(default)]
    isrcs: Vec<String>,
}

/// HTTP client for MusicBrainz WS2 with rate-limit pacing and cache integration.
///
/// Enforces ≥1.1s between outbound requests via `tokio::time::Interval`.
/// Caches both successful resolutions and explicit failures (unresolvable queries).
pub struct MusicBrainzIsrcResolver {
    /// HTTP client configured with user-agent and timeout.
    client: Client,
    /// Shared cache for query results (both successes and explicit failures).
    cache: Arc<Mutex<Cache>>,
    /// Interval enforcing ≥1.1s (or test value) between any two outbound requests.
    interval: Arc<Mutex<Interval>>,
    /// Top-N candidates to filter and look up. Default 10.
    search_limit: u16,
    /// API base URL. Production = `DEFAULT_BASE`; tests override
    /// via `new_with_base` to point at a wiremock server.
    base: String,
}

impl MusicBrainzIsrcResolver {
    /// Create a production resolver with default base URL and 1.1s pacing.
    ///
    /// `cache` is shared so success + explicit-failure persist across queries
    /// within a single run. The caller (Phase 7) is responsible for calling
    /// `Cache::save_atomic` once at the end of the run.
    ///
    /// `user_agent` MUST match the format `order_playlist/<version> (<contact>)`.
    /// Missing/generic UAs are aggressively throttled by MusicBrainz.
    pub fn new(cache: Arc<Mutex<Cache>>, user_agent: String) -> Result<Self, reqwest::Error> {
        Self::new_with_base(
            cache,
            user_agent,
            DEFAULT_BASE.to_string(),
            Duration::from_millis(1100),
        )
    }

    /// Create a resolver with custom base URL and pacing (for testing).
    ///
    /// Used by integration tests to point at a wiremock server and use
    /// minimal pacing (e.g., 1ms) so the test suite completes quickly.
    /// Note: `tokio::time::interval(0)` panics; tests use 1ms as the minimum.
    pub fn new_with_base(
        cache: Arc<Mutex<Cache>>,
        user_agent: String,
        base: String,
        pacing: Duration,
    ) -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(30))
            .build()?;
        let interval = Arc::new(Mutex::new(tokio::time::interval(pacing)));
        Ok(Self {
            client,
            cache,
            interval,
            search_limit: 10,
            base,
        })
    }

    /// Apply pacing by ticking the interval. Blocks for the configured duration.
    async fn pace(&self) {
        let mut interval = self.interval.lock().await;
        interval.tick().await;
    }

    /// Search for a track, filter non-studio recordings, and look up ISRCs.
    ///
    /// Returns `Ok(Some(isrc))` on success, `Ok(None)` when no candidate yielded
    /// an ISRC, and `Err` on network/parse failure. Emits `tracing::info!` on
    /// every network call and `tracing::warn!` on lookup failures.
    async fn resolve_one(&self, query: &TrackQuery) -> Result<Option<TrackId>, MusicBrainzError> {
        self.pace().await;
        let search_url =
            build_search_url(&self.base, &query.title, &query.artist, self.search_limit);

        tracing::info!(
            title = %query.title,
            artist = %query.artist,
            url = %search_url,
            "musicbrainz search"
        );

        let search: SearchResponse = self
            .client
            .get(&search_url)
            .send()
            .await
            .map_err(|e| MusicBrainzError {
                kind: classify_kind(&e),
                query: query.clone(),
                source: Some(e),
            })?
            .error_for_status()
            .map_err(|e| MusicBrainzError {
                kind: classify_kind(&e),
                query: query.clone(),
                source: Some(e),
            })?
            .json()
            .await
            .map_err(|e| MusicBrainzError {
                kind: MusicBrainzErrorKind::Parse,
                query: query.clone(),
                source: Some(e),
            })?;

        // Sort candidates by score descending, then filter to studio-only.
        let mut sorted: Vec<_> = search
            .recordings
            .into_iter()
            .filter(|r| is_studio(&r.disambiguation))
            .collect();
        sorted.sort_by_key(|r| std::cmp::Reverse(r.score));

        // Look up each candidate until we find one with an ISRC.
        for candidate in sorted {
            self.pace().await;
            let lookup_url = build_lookup_url(&self.base, &candidate.id);
            tracing::info!(mbid = %candidate.id, url = %lookup_url, "musicbrainz lookup");

            let lookup: LookupResponse = match self.client.get(&lookup_url).send().await {
                Ok(resp) => match resp.error_for_status() {
                    Ok(r) => r.json().await.map_err(|e| MusicBrainzError {
                        kind: MusicBrainzErrorKind::Parse,
                        query: query.clone(),
                        source: Some(e),
                    })?,
                    Err(e) => {
                        tracing::warn!(mbid = %candidate.id, error = %e, "musicbrainz lookup failed");
                        continue;
                    }
                },
                Err(e) => {
                    tracing::warn!(mbid = %candidate.id, error = %e, "musicbrainz lookup network error");
                    continue;
                }
            };

            if let Some(isrc) = lookup.isrcs.into_iter().next() {
                return Ok(Some(TrackId::new(isrc)));
            }
        }

        Ok(None)
    }
}

/// Classify a `reqwest::Error` into a `MusicBrainzErrorKind`.
///
/// HTTP 503 and 429 are classified as `RateLimit`. Timeout and connection
/// errors are `Network`. JSON decode errors are `Parse`. All others default
/// to `Network`.
fn classify_kind(e: &reqwest::Error) -> MusicBrainzErrorKind {
    if let Some(status) = e.status() {
        if status.as_u16() == 503 || status.as_u16() == 429 {
            return MusicBrainzErrorKind::RateLimit;
        }
    }
    if e.is_timeout() || e.is_connect() {
        return MusicBrainzErrorKind::Network;
    }
    if e.is_decode() {
        return MusicBrainzErrorKind::Parse;
    }
    MusicBrainzErrorKind::Network
}

#[async_trait::async_trait]
impl Resolver for MusicBrainzIsrcResolver {
    /// Resolve multiple queries using cache read-through + search/lookup pipeline.
    ///
    /// For each query:
    /// - Check cache: if cached with non-empty ID, return `Resolved`.
    /// - Check cache: if cached with empty-string sentinel, return `Unresolved` (explicit failure).
    /// - Call `resolve_one`: on `Ok(Some(id))`, cache success and return `Resolved`.
    /// - On `Ok(None)`, cache empty-string sentinel, warn, and return `Unresolved`.
    /// - On `Err`, warn and return `Unresolved` WITHOUT caching (transient error, retry next run).
    async fn resolve_many(&self, queries: &[TrackQuery]) -> Vec<Resolution> {
        let mut out = Vec::with_capacity(queries.len());

        for q in queries {
            // Cache read-through. Clone out the cached value so we don't hold the lock.
            let cached = {
                let cache = self.cache.lock().await;
                cache.get_resolution(q).cloned()
            };

            // The cache stores both successes and explicit failures.
            // A success is a non-empty TrackId.
            // An explicit failure is TrackId("") — the sentinel that says
            // "we tried this and found no ISRC."
            if let Some(id) = cached {
                if id.get().is_empty() {
                    out.push(Resolution::Unresolved {
                        query: q.clone(),
                        reason: "cached: no ISRC found on prior run".into(),
                    });
                } else {
                    out.push(Resolution::Resolved {
                        query: q.clone(),
                        id,
                    });
                }
                continue;
            }

            match self.resolve_one(q).await {
                Ok(Some(id)) => {
                    {
                        let mut cache = self.cache.lock().await;
                        cache.put_resolution(q.clone(), id.clone());
                    }
                    out.push(Resolution::Resolved {
                        query: q.clone(),
                        id,
                    });
                }
                Ok(None) => {
                    {
                        let mut cache = self.cache.lock().await;
                        cache.put_resolution(q.clone(), TrackId::empty());
                    }
                    tracing::warn!(
                        title = %q.title,
                        artist = %q.artist,
                        "unresolved: no ISRC found"
                    );
                    out.push(Resolution::Unresolved {
                        query: q.clone(),
                        reason: "no MusicBrainz candidate had an ISRC".into(),
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        title = %q.title,
                        artist = %q.artist,
                        kind = ?e.kind,
                        "unresolved: error"
                    );
                    // Do NOT cache transient errors — re-attempt on next run.
                    match e.kind {
                        MusicBrainzErrorKind::RateLimit => {
                            out.push(Resolution::ExhaustedRetries { query: q.clone() });
                        }
                        _ => {
                            out.push(Resolution::Unresolved {
                                query: q.clone(),
                                reason: format!("musicbrainz error: {:?}", e.kind),
                            });
                        }
                    }
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Task 4: Unit tests for URL builders and Lucene escape.

    #[test]
    fn escape_lucene_passes_through_safe_chars() {
        assert_eq!(escape_lucene("Hello World"), "Hello World");
    }

    #[test]
    fn escape_lucene_escapes_specials() {
        assert_eq!(escape_lucene("C++ Programming"), "C\\+\\+ Programming");
        assert_eq!(escape_lucene("Run-D.M.C."), "Run\\-D.M.C.");
        assert_eq!(escape_lucene("Love/Hate"), "Love\\/Hate");
    }

    #[test]
    fn build_search_url_includes_lucene_and_limit() {
        let url = build_search_url(DEFAULT_BASE, "Get Lucky", "Daft Punk", 5);
        assert!(url.starts_with("https://musicbrainz.org/ws/2/recording?query="));
        assert!(url.contains("limit=5"));
        assert!(url.contains("fmt=json"));
    }

    #[test]
    fn build_lookup_url_includes_isrcs() {
        let url = build_lookup_url(DEFAULT_BASE, "12345678-1234-1234-1234-123456789abc");
        assert!(url.ends_with("?inc=isrcs&fmt=json"));
        assert!(url.contains("12345678-1234-1234-1234-123456789abc"));
    }

    #[test_case::test_case("live version" ; "live")]
    #[test_case::test_case("remix" ; "remix")]
    #[test_case::test_case("acoustic demo" ; "acoustic demo")]
    #[test_case::test_case("KARAOKE" ; "karaoke uppercase")]
    fn non_studio_disambiguations_filtered(disamb: &str) {
        assert!(!is_studio(disamb));
    }

    #[test_case::test_case("" ; "empty")]
    #[test_case::test_case("explicit" ; "explicit")]
    #[test_case::test_case("featuring Other" ; "featuring")]
    fn studio_disambiguations_kept(disamb: &str) {
        assert!(is_studio(disamb));
    }

    // Task 5: Unit tests for resolver helpers and Send+Sync check.

    #[test]
    fn resolver_is_send_and_sync() {
        fn check<T: Send + Sync>() {}
        check::<MusicBrainzIsrcResolver>();
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Helper to create a resolver pointing at a mock server with minimal pacing.
    async fn resolver_with_mock_base(mock: &MockServer) -> MusicBrainzIsrcResolver {
        let cache = Arc::new(Mutex::new(
            Cache::load(std::path::Path::new("/nonexistent")).unwrap(),
        ));
        MusicBrainzIsrcResolver::new_with_base(
            cache,
            "test/1.0 (test@example.com)".into(),
            mock.uri(),
            std::time::Duration::from_millis(1),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn happy_path_search_then_lookup_returns_isrc() {
        let mock = MockServer::start().await;

        // Search response with one candidate.
        Mock::given(method("GET"))
            .and(path("/recording"))
            .and(query_param("fmt", "json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "recordings": [
                    { "id": "abc-123", "score": 100, "disambiguation": "" }
                ]
            })))
            .mount(&mock)
            .await;

        // Lookup response with one ISRC.
        Mock::given(method("GET"))
            .and(path("/recording/abc-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "isrcs": ["USQX91300120"]
            })))
            .mount(&mock)
            .await;

        let resolver = resolver_with_mock_base(&mock).await;
        let q = TrackQuery::new("Get Lucky", "Daft Punk");
        let result = resolver.resolve_one(&q).await.unwrap();
        assert_eq!(result, Some(TrackId::new("USQX91300120")));
    }

    #[tokio::test]
    async fn filters_non_studio_disambiguation() {
        let mock = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/recording"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "recordings": [
                    { "id": "live-1", "score": 100, "disambiguation": "live version" },
                    { "id": "studio-1", "score": 50, "disambiguation": "" }
                ]
            })))
            .mount(&mock)
            .await;

        // Only the studio recording should be looked up.
        Mock::given(method("GET"))
            .and(path("/recording/studio-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "isrcs": ["USQX91300120"]
            })))
            .mount(&mock)
            .await;

        // The live one MUST NOT be looked up. wiremock's default behavior
        // is to fail the test if an unmounted endpoint is hit.

        let resolver = resolver_with_mock_base(&mock).await;
        let q = TrackQuery::new("Song", "Artist");
        let result = resolver.resolve_one(&q).await.unwrap();
        assert_eq!(result, Some(TrackId::new("USQX91300120")));
    }

    #[tokio::test]
    async fn no_isrcs_returns_none() {
        let mock = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/recording"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "recordings": [
                    { "id": "abc-123", "score": 100, "disambiguation": "" }
                ]
            })))
            .mount(&mock)
            .await;

        Mock::given(method("GET"))
            .and(path("/recording/abc-123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "isrcs": []
            })))
            .mount(&mock)
            .await;

        let resolver = resolver_with_mock_base(&mock).await;
        let q = TrackQuery::new("Get Lucky", "Daft Punk");
        let result = resolver.resolve_one(&q).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn http_503_classified_as_rate_limit() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock)
            .await;

        let resolver = resolver_with_mock_base(&mock).await;
        let q = TrackQuery::new("X", "Y");
        let result = resolver.resolve_one(&q).await;
        let err = result.unwrap_err();
        assert!(matches!(err.kind, MusicBrainzErrorKind::RateLimit));
    }
}
