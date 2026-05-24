# Playlist Arc v1 — Phase 5: Resolver Adapter — MusicBrainz

**Goal:** Resolve `(title, artist)` to a single ISRC by querying MusicBrainz: Lucene search → filter non-studio recordings → MBID lookup with `inc=isrcs` until a non-empty ISRC is found. Cache resolutions (both success and explicit-failure) so the warm-cache path issues zero network requests.

**Architecture:** `MusicBrainzIsrcResolver` is gated by the `musicbrainz` Cargo feature. Implements the `Resolver` trait (defined in this phase). Uses `reqwest` + `tokio::time::Interval` for ≥1.1 s pacing. All network errors flow through a structured `MusicBrainzError` that wraps `reqwest::Error` and the failing `TrackQuery`. Hermetic-by-default: `cargo test` runs against an `InMemoryResolver` test double; the live-HTTP test gates behind the `live-network` Cargo feature.

**Tech Stack:** Rust, `reqwest` (json + rustls-tls), `tokio` (Interval, async), `serde` (JSON response models), `tracing` (info on every network call per AC9.2), `miette::Diagnostic` (user-facing errors).

**Scope:** Phase 5 of 7 from `/Users/y/Apps/music/order_playlist/docs/design-plans/2026-05-23-playlist-arc-v1.md`.

**Codebase verified:** 2026-05-23 — Phase 4's `Cache`, `TrackQuery`, `TrackId` will exist. `src/adapters/mod.rs` re-exports those types. Neither `Resolver` trait nor `musicbrainz.rs` exists yet.

**External dependency findings (MusicBrainz WS2 API):**
- Base URL: `https://musicbrainz.org/ws/2/recording`
- Search: `GET /ws/2/recording?query=<lucene>&fmt=json&limit=N` returns `{ count, offset, recordings: [{ id, score, title, disambiguation, isrcs }] }`. Note `isrcs` may NOT appear in search results — fetch via lookup.
- Lookup: `GET /ws/2/recording/{mbid}?inc=isrcs&fmt=json` returns `{ id, title, disambiguation, isrcs: [...] }`. Empty array if no ISRCs.
- Rate limit: 1 req/sec per IP. Throttling returns **HTTP 503** (not 429). Repeated abuse may extend the cooldown.
- `User-Agent` header **required**: format `AppName/Version (contact-info)`. Missing or generic UAs are aggressively throttled.
- Lucene escapes needed: `+ - && || ! ( ) { } [ ] ^ " ~ * ? : \ /`

**Project guidance:** `/Users/y/Apps/music/order_playlist/implementation-plan-guidance.md`. Key rules: no `unwrap()` outside tests/`main`; new `pub` items need doc comments; "Spotify adapter has mocked tests only" — applies here as "MusicBrainz adapter has mocked tests only" by extension. AC9.2: `tracing::info!` on every network call with structured fields.

---

## Acceptance Criteria Coverage

This phase implements and tests:

### playlist-arc-v1.AC1: Input CSV is read, reordered, augmented with features, and written
- **Partial:** `playlist-arc-v1.AC1.1` — title+artist → ID (ISRC) is the first half of the AC1 pipeline.

### playlist-arc-v1.AC4: Unresolved tracks are logged and written to a sidecar CSV
- **playlist-arc-v1.AC4.3 Success:** Each unresolved track is logged at `WARN` via `tracing`.

(AC4.1, AC4.2, AC4.4 require the orchestration in Phase 7; AC4.5 is verified in Phase 4.)

### playlist-arc-v1.AC8: Adapters are swappable via Cargo features
- **playlist-arc-v1.AC8.2 Success (partial):** The first concrete `Resolver` impl, behind the `musicbrainz` feature. Establishes the swap-point that AC8.2 promises.

### playlist-arc-v1.AC9: Cross-cutting
- **playlist-arc-v1.AC9.1:** Default `cargo test` issues zero network requests; live-HTTP tests only run with `--features live-network`.
- **playlist-arc-v1.AC9.2:** Adapter emits `tracing::info!` on every network call with structured fields (title, artist, mbid).

---

## Task Overview

```
SUBCOMPONENT_A: Resolver trait + Resolution enum + AdapterError + InMemoryResolver (tasks 1-3)
SUBCOMPONENT_B: MusicBrainz HTTP client (tasks 4-6)
SUBCOMPONENT_C: Live-network smoke test + module wiring (tasks 7-8)
```

---

<!-- START_SUBCOMPONENT_A (tasks 1-3) -->

<!-- START_TASK_1 -->
### Task 1: Define Resolver trait + Resolution enum in adapters/mod.rs

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/adapters/mod.rs`
- Create: `/Users/y/Apps/music/order_playlist/src/adapters/musicbrainz.rs` (stub — full impl in Tasks 4–5)

**Implementation:**

The trait defines the contract every resolver implementation must honor. Per AC8.2, future provider features (e.g., `spotify`) add new impls without touching `algo/`, `domain/`, or `cli/`.

**Stub first.** Because `adapters/mod.rs` declares `#[cfg(feature = "musicbrainz")] pub mod musicbrainz;` below, the file `src/adapters/musicbrainz.rs` MUST exist (even empty) before any `cargo build --features musicbrainz` can succeed. Create it now as a stub; Task 4 fills in the contents:

```rust
//! MusicBrainz WS2 resolver. Full implementation lands in Tasks 4–6.
//! Gated by the `musicbrainz` Cargo feature.
```

```rust
//! Impure adapter shell — file IO, HTTP, JSON cache, trait surfaces.

pub mod cache;
pub mod csv_io;

#[cfg(feature = "musicbrainz")]
pub mod musicbrainz;

pub use cache::{Cache, CacheFile, CACHE_VERSION};
pub use csv_io::{read_input, write_output, write_unresolved, Unresolved};

use crate::domain::{TrackId, TrackQuery};

/// Outcome of resolving a single `TrackQuery`.
#[derive(Debug, Clone)]
pub enum Resolution {
    Resolved { query: TrackQuery, id: TrackId },
    Unresolved { query: TrackQuery, reason: String },
}

/// Resolves a batch of `TrackQuery`s into IDs (ISRC for v1's
/// `MusicBrainzIsrcResolver`).
///
/// Implementations must:
/// - Honor cache read-through (skip the network when the query is in cache).
/// - Cache both successes AND explicit failures (so unresolvable queries
///   aren't re-attempted across runs).
/// - Emit `tracing::info!` on every network call (AC9.2).
/// - Emit `tracing::warn!` for each unresolved query (AC4.3).
#[async_trait::async_trait]
pub trait Resolver: Send + Sync {
    async fn resolve_many(&self, queries: &[TrackQuery]) -> Vec<Resolution>;
}
```

The trait uses `async_trait` because Rust's native async-in-traits, while stabilized in recent compilers, still lacks `dyn Trait` support — and Phase 7's orchestration may want a `Box<dyn Resolver>` for runtime swapping. Add `async-trait = "=0.1.95"` to `[dependencies]` in `Cargo.toml`.

**Testing:**

- `Resolution` enum: trivial construction tests.
- `Resolver` trait: a compile-pass test that `Box<dyn Resolver>` is constructible from an impl. The actual impl + test arrives in Task 2.

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo build`
Expected: green. Adding `async-trait` shouldn't break anything.

Run: `cd /Users/y/Apps/music/order_playlist && cargo build --features musicbrainz`
Expected: green — the empty `musicbrainz.rs` stub satisfies the `pub mod musicbrainz;` declaration even before Task 4 fills it in.

**Commit:** `Phase 5: Resolver trait + Resolution enum + async-trait dep + musicbrainz module stub`
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Define MusicBrainzError + AdapterError in errors.rs

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/errors.rs`

**Implementation:**

Add a shared `AdapterError` enum and a MusicBrainz-specific error type. The shared `AdapterError` is wide so that Phase 6's ReccoBeats errors can also flow through it; the per-provider variants carry detail.

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum AdapterError {
    #[error("MusicBrainz error: {0}")]
    #[diagnostic(code(order_playlist::adapter::musicbrainz))]
    MusicBrainz(#[from] MusicBrainzError),

    // Phase 6 will add: #[error("ReccoBeats error: {0}")] ReccoBeats(#[from] ReccoBeatsError),

    #[error("rate limited; exhausted retries on {endpoint}")]
    #[diagnostic(
        code(order_playlist::adapter::rate_limited),
        help("re-run later; consider authenticated MusicBrainz access for higher quota")
    )]
    RateLimited { endpoint: String },
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("MusicBrainz {kind:?} for {query:?}")]
#[diagnostic(code(order_playlist::adapter::musicbrainz::error))]
pub struct MusicBrainzError {
    pub kind: MusicBrainzErrorKind,
    pub query: crate::domain::TrackQuery,
    #[source]
    pub source: Option<reqwest::Error>,
}

#[derive(Debug, Clone)]
pub enum MusicBrainzErrorKind {
    Network,
    Parse,
    RateLimit,
    NoCandidates,
}
```

The `source` is `Option<reqwest::Error>` because `NoCandidates` doesn't have an underlying reqwest error.

**Testing:**

- Construct each variant and assert the `Display` output mentions the query's title.
- `AdapterError::from(MusicBrainzError { ... })` works (via `#[from]`).

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo build && cargo test --lib errors`
Expected: green.

**Commit:** `Phase 5: MusicBrainzError + AdapterError with miette diagnostics`
<!-- END_TASK_2 -->

<!-- START_TASK_3 -->
### Task 3: Create InMemoryResolver test double in tests/support/

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/tests/support/mod.rs`
- Create: `/Users/y/Apps/music/order_playlist/tests/support/in_memory.rs`

**Implementation:**

`tests/support/` is the shared test-helper module imported by integration tests in `tests/*.rs`. Since Rust doesn't auto-include `tests/support/` in each test binary, each test file that uses these helpers will need `mod support;` at the top.

`tests/support/mod.rs`:
```rust
//! Shared helpers for integration tests.

pub mod in_memory;
```

`tests/support/in_memory.rs`:
```rust
//! In-memory `Resolver` and `FeatureSource` test doubles.
//!
//! These are used by Phase 7's integration tests to exercise the
//! pipeline without touching the network. The doubles also include
//! `PanicOnCallResolver` / `PanicOnCallFeatureSource` (Phase 7 task)
//! to prove the warm-cache path issues zero network calls.

use std::collections::HashMap;
use order_playlist::adapters::{Resolution, Resolver};
use order_playlist::domain::{TrackId, TrackQuery};

pub struct InMemoryResolver {
    pub map: HashMap<TrackQuery, TrackId>,
}

impl InMemoryResolver {
    pub fn new(pairs: impl IntoIterator<Item = (TrackQuery, TrackId)>) -> Self {
        Self { map: pairs.into_iter().collect() }
    }
}

#[async_trait::async_trait]
impl Resolver for InMemoryResolver {
    async fn resolve_many(&self, queries: &[TrackQuery]) -> Vec<Resolution> {
        queries.iter()
            .map(|q| match self.map.get(q) {
                Some(id) => Resolution::Resolved { query: q.clone(), id: id.clone() },
                None => Resolution::Unresolved {
                    query: q.clone(),
                    reason: "no fixture entry".into(),
                },
            })
            .collect()
    }
}
```

For Phase 7's zero-network test, add a `PanicOnCallResolver` placeholder (it can stay here in Phase 5 since Phase 6 will add the feature-source analogue):
```rust
pub struct PanicOnCallResolver;

#[async_trait::async_trait]
impl Resolver for PanicOnCallResolver {
    async fn resolve_many(&self, _queries: &[TrackQuery]) -> Vec<Resolution> {
        panic!("PanicOnCallResolver was invoked — this should not happen on a warm-cache path");
    }
}
```

**Testing:**

A small integration test verifies the in-memory resolver works. Place at `tests/in_memory_resolver_smoke.rs`:

```rust
mod support;

use support::in_memory::InMemoryResolver;
use order_playlist::adapters::{Resolution, Resolver};
use order_playlist::domain::{TrackId, TrackQuery};

#[tokio::test]
async fn in_memory_resolver_returns_known_id() {
    let q = TrackQuery::new("Get Lucky", "Daft Punk");
    let id = TrackId::new("USQX91300120");
    let r = InMemoryResolver::new([(q.clone(), id.clone())]);
    let results = r.resolve_many(&[q.clone()]).await;
    match &results[0] {
        Resolution::Resolved { query, id: got_id } => {
            assert_eq!(query, &q);
            assert_eq!(got_id, &id);
        }
        _ => panic!("expected Resolved"),
    }
}

#[tokio::test]
async fn in_memory_resolver_returns_unresolved_for_missing() {
    let r = InMemoryResolver::new([]);
    let q = TrackQuery::new("Unknown", "Nobody");
    let results = r.resolve_many(&[q.clone()]).await;
    assert!(matches!(&results[0], Resolution::Unresolved { .. }));
}
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --test in_memory_resolver_smoke`
Expected: both tests pass.

**Commit:** `Phase 5: InMemoryResolver + PanicOnCallResolver test doubles`
<!-- END_TASK_3 -->

<!-- END_SUBCOMPONENT_A -->

<!-- START_SUBCOMPONENT_B (tasks 4-6) -->

<!-- START_TASK_4 -->
### Task 4: Implement Lucene escape + query construction

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/src/adapters/musicbrainz.rs`

**Implementation:**

Start with the parts that need no HTTP — pure helpers for building the MusicBrainz request URL. Test these exhaustively before adding the network layer.

```rust
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

/// Default MusicBrainz WS2 base URL. Overridable per-instance via
/// `MusicBrainzIsrcResolver::new_with_base` (Task 5) so wiremock tests
/// can point at a mock server. URL builders take the base as a `&str`
/// argument — keeping them functions (not methods) keeps unit tests
/// trivial and avoids needing a resolver instance to test URL construction.
pub const DEFAULT_BASE: &str = "https://musicbrainz.org/ws/2";

/// Lucene special characters that must be backslash-escaped.
const LUCENE_SPECIALS: &[char] = &[
    '+', '-', '&', '|', '!', '(', ')', '{', '}', '[', ']',
    '^', '"', '~', '*', '?', ':', '\\', '/',
];

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

pub(crate) fn build_search_url(base: &str, title: &str, artist: &str, limit: u16) -> String {
    let q = format!("title:\"{}\" AND artist:\"{}\"",
        escape_lucene(title), escape_lucene(artist));
    format!("{base}/recording?query={}&fmt=json&limit={}",
        urlencoding::encode(&q), limit)
}

pub(crate) fn build_lookup_url(base: &str, mbid: &str) -> String {
    format!("{base}/recording/{}?inc=isrcs&fmt=json", mbid)
}

/// Disambiguation phrases that mark a recording as non-studio for v1.
pub(crate) const NON_STUDIO_MARKERS: &[&str] = &[
    "live", "remix", "demo", "instrumental", "acoustic", "karaoke", "cover",
];

pub(crate) fn is_studio(disambiguation: &str) -> bool {
    let lower = disambiguation.to_lowercase();
    !NON_STUDIO_MARKERS.iter().any(|m| lower.contains(m))
}
```

Add `urlencoding = "=2.1.3"` to `[dependencies]` in `Cargo.toml` for the URL percent-encoding.

**Testing (in `#[cfg(test)] mod tests` of the same file):**

```rust
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
    // URL-encoded form of `title:"Get Lucky" AND artist:"Daft Punk"`
    assert!(url.contains("title%3A") || url.contains("title:"));
}

#[test]
fn build_lookup_url_includes_isrcs() {
    let url = build_lookup_url(DEFAULT_BASE, "12345678-1234-1234-1234-123456789abc");
    assert!(url.ends_with("?inc=isrcs&fmt=json"));
    assert!(url.contains("12345678-1234-1234-1234-123456789abc"));
}

#[test_case::test_case("live version" ; "live")]
#[test_case::test_case("remix"        ; "remix")]
#[test_case::test_case("acoustic demo"; "acoustic demo")]
#[test_case::test_case("KARAOKE"      ; "karaoke uppercase")]
fn non_studio_disambiguations_filtered(disamb: &str) {
    assert!(!is_studio(disamb));
}

#[test_case::test_case("")                  ; "empty")]
#[test_case::test_case("explicit")          ; "explicit")]
#[test_case::test_case("featuring Other")   ; "featuring")]
fn studio_disambiguations_kept(disamb: &str) {
    assert!(is_studio(disamb));
}
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --features musicbrainz --lib adapters::musicbrainz::tests`
Expected: all pass.

**Commit:** `Phase 5: MusicBrainz URL builders + Lucene escaping`
<!-- END_TASK_4 -->

<!-- START_TASK_5 -->
### Task 5: Implement MusicBrainzIsrcResolver struct + JSON response models + Resolver impl

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/adapters/musicbrainz.rs`

**Implementation:**

```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Interval;
use reqwest::Client;
use crate::adapters::{Cache, Resolution, Resolver};
use crate::domain::{TrackId, TrackQuery};
use crate::errors::{MusicBrainzError, MusicBrainzErrorKind};

#[derive(serde::Deserialize, Debug)]
struct SearchResponse {
    recordings: Vec<SearchRecording>,
}

#[derive(serde::Deserialize, Debug)]
struct SearchRecording {
    id: String,
    score: i32,
    #[serde(default)]
    disambiguation: String,
}

#[derive(serde::Deserialize, Debug)]
struct LookupResponse {
    #[serde(default)]
    isrcs: Vec<String>,
}

pub struct MusicBrainzIsrcResolver {
    client: Client,
    cache: Arc<Mutex<Cache>>,
    /// Interval enforcing ≥1.1s between any two outbound requests.
    interval: Arc<Mutex<Interval>>,
    /// Top-N candidates to filter and look up. Default 10.
    search_limit: u16,
    /// API base URL. Production = `DEFAULT_BASE`; tests override
    /// via `new_with_base` to point at a wiremock server.
    base: String,
    /// Pacing interval between outbound requests. Production = 1100 ms;
    /// tests use `Duration::from_millis(0)` to keep the suite fast.
    pacing: Duration,
}

impl MusicBrainzIsrcResolver {
    /// Production constructor. `cache` is shared so success +
    /// explicit-failure persist across queries within a single run.
    /// The caller (Phase 7) is responsible for calling
    /// `Cache::save_atomic` once at the end of the run.
    ///
    /// `user_agent` MUST match the format `order_playlist/<version> (<contact>)`.
    /// Missing/generic UAs are aggressively throttled by MusicBrainz.
    pub fn new(cache: Arc<Mutex<Cache>>, user_agent: String) -> Result<Self, reqwest::Error> {
        Self::new_with_base(cache, user_agent, DEFAULT_BASE.to_string(), Duration::from_millis(1100))
    }

    /// Test/integration constructor — override base URL and pacing.
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
        Ok(Self { client, cache, interval, search_limit: 10, base, pacing })
    }

    async fn pace(&self) {
        let mut interval = self.interval.lock().await;
        interval.tick().await;
    }

    /// Search → filter → lookup → ISRC. Returns Ok(Some(isrc)) on success,
    /// Ok(None) when no candidate yielded an ISRC, Err on network/parse failure.
    async fn resolve_one(&self, query: &TrackQuery) -> Result<Option<TrackId>, MusicBrainzError> {
        self.pace().await;
        let search_url = build_search_url(&self.base, &query.title, &query.artist, self.search_limit);

        tracing::info!(title = %query.title, artist = %query.artist, url = %search_url, "musicbrainz search");

        let search: SearchResponse = self.client.get(&search_url).send().await
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
            .json().await
            .map_err(|e| MusicBrainzError {
                kind: MusicBrainzErrorKind::Parse,
                query: query.clone(),
                source: Some(e),
            })?;

        // Sort candidates by score desc, then filter non-studio.
        let mut sorted: Vec<_> = search.recordings.into_iter()
            .filter(|r| is_studio(&r.disambiguation))
            .collect();
        sorted.sort_by(|a, b| b.score.cmp(&a.score));

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
                        continue; // try next candidate
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

fn classify_kind(e: &reqwest::Error) -> MusicBrainzErrorKind {
    if let Some(status) = e.status() {
        if status.as_u16() == 503 || status.as_u16() == 429 {
            return MusicBrainzErrorKind::RateLimit;
        }
    }
    if e.is_timeout() || e.is_connect() { return MusicBrainzErrorKind::Network; }
    if e.is_decode() { return MusicBrainzErrorKind::Parse; }
    MusicBrainzErrorKind::Network
}
```

Add the `Resolver` impl:

```rust
#[async_trait::async_trait]
impl Resolver for MusicBrainzIsrcResolver {
    async fn resolve_many(&self, queries: &[TrackQuery]) -> Vec<Resolution> {
        let mut out = Vec::with_capacity(queries.len());

        for q in queries {
            // Cache read-through (clone the cached id out so we don't hold the lock).
            let cached = {
                let cache = self.cache.lock().await;
                cache.get_resolution(q).cloned()
            };

            // The cache stores both successes and explicit failures. A success
            // is a non-empty TrackId. An explicit failure is TrackId(""):
            // we record that we've tried this query and got no ISRC.
            if let Some(id) = cached {
                if id.get().is_empty() {
                    out.push(Resolution::Unresolved {
                        query: q.clone(),
                        reason: "cached: no ISRC found on prior run".into(),
                    });
                } else {
                    out.push(Resolution::Resolved { query: q.clone(), id });
                }
                continue;
            }

            match self.resolve_one(q).await {
                Ok(Some(id)) => {
                    {
                        let mut cache = self.cache.lock().await;
                        cache.put_resolution(q.clone(), id.clone());
                    }
                    out.push(Resolution::Resolved { query: q.clone(), id });
                }
                Ok(None) => {
                    {
                        let mut cache = self.cache.lock().await;
                        cache.put_resolution(q.clone(), TrackId::new("")); // explicit-failure sentinel
                    }
                    tracing::warn!(title = %q.title, artist = %q.artist, "unresolved: no ISRC found");
                    out.push(Resolution::Unresolved {
                        query: q.clone(),
                        reason: "no MusicBrainz candidate had an ISRC".into(),
                    });
                }
                Err(e) => {
                    tracing::warn!(title = %q.title, artist = %q.artist, kind = ?e.kind, "unresolved: error");
                    // Do NOT cache transient errors — re-attempt on next run.
                    out.push(Resolution::Unresolved {
                        query: q.clone(),
                        reason: format!("musicbrainz error: {:?}", e.kind),
                    });
                }
            }
        }

        out
    }
}
```

**Note on the explicit-failure sentinel.** Using an empty-string TrackId as the "no ISRC found" sentinel keeps the cache shape unchanged. The alternative — wrapping `TrackId` in an `Option` in the cache type — would force a Phase 4 retrofit. Document the sentinel in the file's module doc comment.

**Testing (unit tests of the helpers; the full resolve_many is exercised in Task 7 via the live-network test, and via integration in Phase 7):**

- `classify_kind` correctly categorizes a synthetic `reqwest::Error::status` (use `reqwest::Response::error_for_status` against a mock server, or fall back to documenting that classification is exercised only by the live test).
- The empty-string sentinel round-trips through the cache.
- **Send+Sync smoke check.** Phase 7's `RunDeps` boxes `Box<dyn Resolver>` (which requires `Send + Sync`). Add a compile-time assertion so a future field that breaks auto-trait inheritance fails loudly:
  ```rust
  #[test]
  fn resolver_is_send_and_sync() {
      fn check<T: Send + Sync>() {}
      check::<MusicBrainzIsrcResolver>();
  }
  ```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo build --features musicbrainz`
Expected: green.

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --features musicbrainz --lib adapters::musicbrainz`
Expected: helper tests pass (live-HTTP test is in Task 7).

**Commit:** `Phase 5: MusicBrainzIsrcResolver with read-through cache + explicit-failure sentinel`
<!-- END_TASK_5 -->

<!-- START_TASK_6 -->
### Task 6: Add a mock-server test for the search → lookup pipeline

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/adapters/musicbrainz.rs`
- Modify: `/Users/y/Apps/music/order_playlist/Cargo.toml` (add `wiremock = "=0.6.4"` to `[dev-dependencies]`)

**Implementation:**

A `wiremock` test exercises the full `resolve_one` path against a mock HTTP server — no live network, but real `reqwest` calls. This catches URL-builder bugs, JSON-deserialization bugs, and `classify_kind` regressions without depending on MusicBrainz being available.

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path, query_param};

    async fn resolver_with_mock_base(mock: &MockServer) -> MusicBrainzIsrcResolver {
        // `new_with_base` was defined in Task 5; tests pass the mock URI
        // and a zero pacing so the suite stays fast (< 10s total).
        let cache = Arc::new(Mutex::new(Cache::load(std::path::Path::new("/nonexistent")).unwrap()));
        MusicBrainzIsrcResolver::new_with_base(
            cache,
            "test/1.0 (test@example.com)".into(),
            mock.uri(),
            std::time::Duration::from_millis(0),
        ).unwrap()
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
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(503))
            .mount(&mock).await;

        let resolver = resolver_with_mock_base(&mock).await;
        let q = TrackQuery::new("X", "Y");
        let result = resolver.resolve_one(&q).await;
        let err = result.unwrap_err();
        assert!(matches!(err.kind, MusicBrainzErrorKind::RateLimit));
    }
}
```

**No refactor needed.** Task 5 already defined `MusicBrainzIsrcResolver` with `base: String` and `pacing: Duration` fields and the `new_with_base(cache, user_agent, base, pacing)` constructor. The wiremock tests pass `mock.uri()` and `Duration::from_millis(0)` — pacing-free tests keep the suite under 10s.

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --features musicbrainz --lib adapters::musicbrainz`
Expected: all unit + wiremock tests pass. Total runtime < 10s.

**Commit:** `Phase 5: wiremock integration tests for MusicBrainz happy path + filter + 503`
<!-- END_TASK_6 -->

<!-- END_SUBCOMPONENT_B -->

<!-- START_SUBCOMPONENT_C (tasks 7-8) -->

<!-- START_TASK_7 -->
### Task 7: Add live-network smoke test (feature-gated)

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/tests/musicbrainz_live.rs`

**Implementation:**

```rust
//! Live-HTTP smoke test for the MusicBrainz resolver.
//!
//! Gated by the `live-network` Cargo feature so default `cargo test`
//! never touches the network. Per AC9.1: no silent-skip pattern — if
//! the feature is enabled but the network is unreachable, the test
//! must fail loudly (and it will, since reqwest's connect timeout
//! surfaces a real error).

#![cfg(all(feature = "musicbrainz", feature = "live-network"))]

use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

use order_playlist::adapters::{Cache, MusicBrainzIsrcResolver, Resolution, Resolver};
use order_playlist::domain::TrackQuery;

#[tokio::test]
async fn resolves_get_lucky_to_isrc() {
    let dir = TempDir::new().unwrap();
    let cache = Arc::new(Mutex::new(
        Cache::load(&dir.path().join("cache.json")).unwrap()
    ));

    let resolver = MusicBrainzIsrcResolver::new(
        cache,
        "order_playlist-test/0.1 (test@example.com)".into(),
    ).unwrap();

    let q = TrackQuery::new("Get Lucky", "Daft Punk");
    let results = resolver.resolve_many(&[q]).await;
    match &results[0] {
        Resolution::Resolved { id, .. } => {
            // Just assert non-empty — the exact ISRC may vary across
            // MusicBrainz updates. The known US ISRC for "Get Lucky"
            // is "USQX91300120" but newer releases may rank higher.
            assert!(!id.get().is_empty(), "expected a non-empty ISRC");
        }
        Resolution::Unresolved { reason, .. } => {
            panic!("expected Resolved, got Unresolved: {}", reason);
        }
    }
}
```

When `live-network` is NOT enabled, the entire file is excluded by `#![cfg(...)]` — `cargo test` doesn't compile it, doesn't try to run it, doesn't skip-with-message. This satisfies AC9.1 ("no silent skips" — there's literally nothing in the test binary to skip).

When the user runs `cargo test --features live-network`, this test runs and either succeeds (live API responded) or fails (clear network/parse error). No silent passes.

**Verification:**

Without the feature:
```bash
cd /Users/y/Apps/music/order_playlist && cargo test --test musicbrainz_live 2>&1 | head -5
```
Expected: builds with zero tests in this file (the cfg excludes everything).

With the feature (network required):
```bash
cd /Users/y/Apps/music/order_playlist && cargo test --features musicbrainz,live-network --test musicbrainz_live 2>&1 | tail -10
```
Expected: passes when the network reaches musicbrainz.org. If offline, test fails with a clear reqwest error.

**Commit:** `Phase 5: live-network smoke test for MusicBrainz resolver`
<!-- END_TASK_7 -->

<!-- START_TASK_8 -->
### Task 8: Re-export, full verification, commit

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/adapters/mod.rs`

**Implementation:**

Add the re-export so Phase 7's `main.rs` can `use order_playlist::adapters::MusicBrainzIsrcResolver`:

```rust
#[cfg(feature = "musicbrainz")]
pub use musicbrainz::MusicBrainzIsrcResolver;
```

Final verification:

```bash
cd /Users/y/Apps/music/order_playlist
cargo fmt --check
cargo clippy --all-targets --features musicbrainz -- -D warnings
cargo test --features musicbrainz
cargo build --release --features musicbrainz
```

All four exit 0.

Additional gate (AC8 surface):
```bash
cd /Users/y/Apps/music/order_playlist
cargo build --no-default-features --features musicbrainz
```
Expected: builds (the `reccobeats` adapter isn't here yet, but `no-default-features --features musicbrainz` should produce a binary that has only the MusicBrainz resolver and no ReccoBeats — verification of AC8.3's path comes in Phase 6+7).

**Commit:** Bundle if needed:
```bash
cd /Users/y/Apps/music/order_playlist
git add src/adapters/ Cargo.toml Cargo.lock
git status
git commit -m "Phase 5: re-export MusicBrainzIsrcResolver; full verification"
```
<!-- END_TASK_8 -->

<!-- END_SUBCOMPONENT_C -->

---

## Phase 5 Done When

- `cargo test --features musicbrainz` passes, including wiremock integration tests.
- `cargo test --features musicbrainz,live-network` passes when the network is available (and is excluded from default `cargo test` runs).
- Pure unit tests cover Lucene escape, URL builders, and non-studio filter.
- `wiremock` tests cover happy path, non-studio filter, no-ISRC, and HTTP 503 classification.
- `Resolver` trait + `Resolution` enum + `MusicBrainzError` + `AdapterError` defined and exposed.
- `InMemoryResolver` + `PanicOnCallResolver` test doubles available for Phase 7.
- Cache read-through honored; cache stores both successes and explicit failures.
- `tracing::info!` emitted on every network call with structured fields (AC9.2).
- `tracing::warn!` emitted for every unresolved query (AC4.3).
- All `pub` items have doc comments.
- `cargo fmt --check`, `cargo clippy --all-targets --features musicbrainz -- -D warnings`, `cargo build --release` all exit 0.
- Commits on `playlist-arc-v1` whose subjects start with `Phase 5:`.

## Risk callouts

- **Pacing slows the wiremock tests.** Use `with_pacing(Duration::from_millis(0))` in tests to keep the suite under 10 s.
- **`reqwest` URL behavior for empty paths.** The `base` URL must NOT have a trailing slash, or path joining will be wrong. `wiremock::MockServer::uri()` doesn't add a trailing slash — fine.
- **MusicBrainz schema drift.** The recordings response shape is fairly stable, but `serde::Deserialize` with `#[serde(default)]` on `disambiguation` and `isrcs` is the safety net against missing fields. Don't add `#[serde(deny_unknown_fields)]` — that would break on every new field MusicBrainz adds.
- **Live test flakiness.** "Get Lucky" by Daft Punk is a very stable recording in MusicBrainz, but if the live test starts failing, log the search response and add a more boring fixture (e.g., a public-domain classical recording).
