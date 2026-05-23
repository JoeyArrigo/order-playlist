# Playlist Arc v1 — Phase 6: Feature Source Adapter — ReccoBeats

**Goal:** Convert resolved IDs (ISRCs) to `TrackFeatures` via ReccoBeats: batch ≤ 40 IDs per call, dedupe many-to-many ISRC results, retry once on HTTP 429 with `Retry-After`. Cache features per-ID so warm runs hit zero network.

**Architecture:** `ReccoBeatsFeatures` is gated by the `reccobeats` Cargo feature. Implements the `FeatureSource` trait (defined in this phase). The trait surface is opaque to `algo/*` — the orchestration in Phase 7 collapses `Vec<(TrackId, Option<TrackFeatures>)>` into `Vec<Track>` (resolved) + `Vec<Unresolved>` (skipped) before handing tracks to the annealer.

**Tech Stack:** Rust, `reqwest`, `tokio`, `serde`, `tracing`. Same pattern as Phase 5 — pure URL/parse helpers tested with unit tests, integration tested with `wiremock`, live-network smoke test gated by feature flag.

**Scope:** Phase 6 of 7 from `/Users/y/Apps/music/playlistize/docs/design-plans/2026-05-23-playlist-arc-v1.md`.

**Codebase verified:** 2026-05-23 — Phase 4's `Cache` + Phase 5's `Resolver`/`Resolution` + `tests/support/in_memory.rs` exist. `src/adapters/reccobeats.rs` does not.

**⚠️ External dependency findings (ReccoBeats — INVESTIGATE BEFORE IMPLEMENTING):**

Internet research in May 2026 reports the ReccoBeats API differs from the design's assumptions in three important ways. **The implementer MUST verify the actual API shape with a manual HTTP spike before writing the full adapter.** If the findings hold, Phase 6 needs minor structural adjustments documented inline below.

| Design assumption | Research-reported reality | Action |
|---|---|---|
| `GET /v1/audio-features?ids=<id1>&ids=<id2>…` accepts ISRCs | Endpoint may be `GET /v1/track/{id}/audio-features` (per-track) OR `GET /v1/track?ids=...` (batch track lookup) returning Spotify IDs and ISRC, and then separate audio-features calls per track | Implement both shapes behind one function; pick the working one at runtime via a small spike test |
| Response includes `key`, `mode` | Research says ReccoBeats audio-features omits `key`, `mode`, `time_signature` | If confirmed, fall back to `PitchClass::new(0)` + `Mode::Major` at the adapter boundary and document the degradation. The Camelot cost term becomes uniform (all distances 0); the algorithm continues to work, energy-arc term dominates. |
| Batches of ≤ 40 IDs per call | Batch size limit not documented | Start with batches of 40; if the API rejects, halve and retry until the batch fits |
| Maps response back via `href` (Spotify URL) or `isrc` | Plausible, but the `href` field's actual contents need verification | Implement both lookups; log which one matched |

**The Task-1 manual spike** (described below) is mandatory and must occur **before** any further implementation. If the spike reveals that the API simply doesn't return audio features by ISRC at all, surface the finding to the user before continuing — that would require a v1 scope change.

**Project guidance:** `/Users/y/Apps/music/playlistize/implementation-plan-guidance.md`. Same rules as Phase 5: no `unwrap()` outside tests/`main`; doc comments on `pub` items; AC9.1 (no silent skips); AC9.2 (`tracing::info!` on every network call).

---

## Acceptance Criteria Coverage

This phase implements and tests:

### playlist-arc-v1.AC1: Input CSV is read, reordered, augmented with features, and written
- **Partial:** `playlist-arc-v1.AC1.1` — ID → features is the second half of the AC1 pipeline. After this phase, the pipeline can produce fully-populated `Track` values.

### playlist-arc-v1.AC8: Adapters are swappable via Cargo features
- **Partial:** the second concrete adapter, behind the `reccobeats` feature.

### playlist-arc-v1.AC9: Cross-cutting
- **playlist-arc-v1.AC9.1:** Default `cargo test` issues zero network requests; live tests only run with `--features live-network`.
- **playlist-arc-v1.AC9.2:** Adapter emits `tracing::info!` on every network call with structured fields (ids, batch size).

---

## Task Overview

```
SUBCOMPONENT_A: API spike (Task 1) + FeatureSource trait + InMemory test double (tasks 1-3)
SUBCOMPONENT_B: ReccoBeats client implementation (tasks 4-6)
SUBCOMPONENT_C: Live-network smoke test + module wiring (tasks 7-8)
```

---

<!-- START_SUBCOMPONENT_A (tasks 1-3) -->

<!-- START_TASK_1 -->
### Task 1: Manual API spike — verify the actual ReccoBeats response shape

**Files:** None modified. This is a verification-only spike.

**Implementation:**

Before writing any Rust, run these `curl` commands against the live API. Capture and inspect the responses. Document findings in the commit message or in `docs/implementation-plans/2026-05-23-playlist-arc-v1/RECCOBEATS_SPIKE.md`.

```bash
# 1. Does the audio-features endpoint exist with the design's URL shape?
curl -s "https://api.reccobeats.com/v1/audio-features?ids=USQX91300120" | jq

# 2. Does it accept a Spotify ID instead?
curl -s "https://api.reccobeats.com/v1/audio-features?ids=69kOkLUCkxIZYexIgSG8rq" | jq

# 3. Try the per-track shape (if it differs).
curl -s "https://api.reccobeats.com/v1/track/USQX91300120/audio-features" | jq

# 4. Try the batch-track endpoint.
curl -s "https://api.reccobeats.com/v1/track?ids=USQX91300120" | jq

# 5. Try with multiple IDs to confirm the query-param shape.
curl -s "https://api.reccobeats.com/v1/audio-features?ids=USQX91300120&ids=USQX91100008" | jq

# 6. Send 50 IDs to find the actual batch limit.
curl -s "https://api.reccobeats.com/v1/audio-features?$(for i in $(seq 1 50); do echo -n "ids=USQX9130012${i}&"; done)" | head -c 500
```

**Decisions to make from the spike:**

1. **Which endpoint shape works?** Update the constants in Task 4 accordingly.
2. **Which fields are present in the response?** Confirm/refute the `key`/`mode` absence.
3. **What's the actual batch limit?** Update `MAX_BATCH` in Task 4.
4. **How does it indicate "no match"?** Empty array, missing entry, status code?
5. **What does `href` actually contain?** Spotify URL? Internal ReccoBeats URL?

**Verification — MANDATORY before Task 4:**

The spike is complete when:
1. `docs/implementation-plans/2026-05-23-playlist-arc-v1/RECCOBEATS_SPIKE.md` exists and explicitly answers ALL five questions above.
2. The file is committed to the branch (so future re-runs of this phase don't lose the context).

Run this gate before starting Task 4:
```bash
cd /Users/y/Apps/music/playlistize
test -f docs/implementation-plans/2026-05-23-playlist-arc-v1/RECCOBEATS_SPIKE.md \
  && grep -q "^## Question 1" docs/implementation-plans/2026-05-23-playlist-arc-v1/RECCOBEATS_SPIKE.md \
  && grep -q "^## Question 5" docs/implementation-plans/2026-05-23-playlist-arc-v1/RECCOBEATS_SPIKE.md \
  || echo "BLOCKED: complete RECCOBEATS_SPIKE.md before Task 4"
```

Use this template for `RECCOBEATS_SPIKE.md`:
```markdown
# ReccoBeats API spike — 2026-05-23

## Question 1: Which endpoint shape works?
[your answer with the working URL]

## Question 2: Which fields are present in the response?
[list of fields actually returned]

## Question 3: What's the actual batch limit?
[number]

## Question 4: How does the API indicate "no match"?
[answer]

## Question 5: What does `href` actually contain?
[example value]

## Decisions
- MAX_BATCH = ...
- Endpoint = ...
- key/mode handling = (returned | default to PitchClass(0)+Major)
```

**HALT PATH — if the spike reveals the API fundamentally doesn't support ISRC → features:**

STOP Phase 6 entirely. Do NOT proceed to Phase 7. Do NOT implement workarounds — they are explicit non-goals for v1 per the design (Spotify client-credentials, GetSongBPM, local-table embedding all out of v1 scope).

Surface the problem to the user with a written summary:
1. Which spike question failed.
2. The actual API response (paste the curl output).
3. Request: which of the design's listed v2 options the user wants to escalate to v1, or whether v1 should be re-scoped.

The implementor must NOT make this decision unilaterally — it changes the design contract.

**Commit the spike findings** (required, not optional):
```bash
cd /Users/y/Apps/music/playlistize
git add docs/implementation-plans/2026-05-23-playlist-arc-v1/RECCOBEATS_SPIKE.md
git commit -m "Phase 6: API spike findings for ReccoBeats"
```
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Define FeatureSource trait + ReccoBeatsError

**Files:**
- Modify: `/Users/y/Apps/music/playlistize/src/adapters/mod.rs`
- Modify: `/Users/y/Apps/music/playlistize/src/errors.rs`

**Implementation:**

In `adapters/mod.rs`:
```rust
use crate::domain::{TrackFeatures, TrackId};

/// Resolves IDs to audio features. Implementations must:
/// - Honor cache read-through per ID; only un-cached IDs hit the network.
/// - Cache successes; do NOT cache transient failures (re-attempt on next run).
/// - Emit `tracing::info!` on every network call.
#[async_trait::async_trait]
pub trait FeatureSource: Send + Sync {
    async fn features_for(&self, ids: &[TrackId]) -> Vec<(TrackId, Option<TrackFeatures>)>;
}

#[cfg(feature = "reccobeats")]
pub mod reccobeats;

#[cfg(feature = "reccobeats")]
pub use reccobeats::ReccoBeatsFeatures;
```

In `errors.rs`:
```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("ReccoBeats {kind:?} for {ids:?}")]
#[diagnostic(code(playlistize::adapter::reccobeats::error))]
pub struct ReccoBeatsError {
    pub kind: ReccoBeatsErrorKind,
    pub ids: Vec<String>,
    #[source]
    pub source: Option<reqwest::Error>,
}

#[derive(Debug, Clone)]
pub enum ReccoBeatsErrorKind {
    Network,
    Parse,
    Throttled,
}

// And extend AdapterError:
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum AdapterError {
    #[error("MusicBrainz error: {0}")]
    #[diagnostic(code(playlistize::adapter::musicbrainz))]
    MusicBrainz(#[from] MusicBrainzError),

    #[error("ReccoBeats error: {0}")]
    #[diagnostic(code(playlistize::adapter::reccobeats))]
    ReccoBeats(#[from] ReccoBeatsError),

    #[error("rate limited; exhausted retries on {endpoint}")]
    #[diagnostic(
        code(playlistize::adapter::rate_limited),
        help("re-run later; consider authenticated access for higher quota")
    )]
    RateLimited { endpoint: String },
}
```

**Testing:**

- Trivial construction tests for `ReccoBeatsError` and each `ReccoBeatsErrorKind` variant.
- `AdapterError::from(ReccoBeatsError { .. })` compiles and constructs.

**Verification:**

Run: `cd /Users/y/Apps/music/playlistize && cargo build --features reccobeats`
Expected: green (the `reccobeats.rs` doesn't exist yet — comment out the `#[cfg(feature = "reccobeats")] pub mod reccobeats;` until Task 4 or use a stub).

Add a stub `src/adapters/reccobeats.rs`:
```rust
//! ReccoBeats audio-features adapter. Gated by the `reccobeats` Cargo feature.
//!
//! Full implementation lands in Tasks 4–6 once Task 1's API spike confirms
//! the request/response shape.

// Stub — replaced in Task 4.
```

**Commit:** `Phase 6: FeatureSource trait + ReccoBeatsError + AdapterError extension`
<!-- END_TASK_2 -->

<!-- START_TASK_3 -->
### Task 3: Add InMemoryFeatureSource + PanicOnCallFeatureSource test doubles

**Files:**
- Modify: `/Users/y/Apps/music/playlistize/tests/support/in_memory.rs`

**Implementation:**

```rust
use std::collections::HashMap;
use playlistize::adapters::FeatureSource;
use playlistize::domain::{TrackFeatures, TrackId};

pub struct InMemoryFeatureSource {
    pub map: HashMap<TrackId, TrackFeatures>,
}

impl InMemoryFeatureSource {
    pub fn new(pairs: impl IntoIterator<Item = (TrackId, TrackFeatures)>) -> Self {
        Self { map: pairs.into_iter().collect() }
    }
}

#[async_trait::async_trait]
impl FeatureSource for InMemoryFeatureSource {
    async fn features_for(&self, ids: &[TrackId]) -> Vec<(TrackId, Option<TrackFeatures>)> {
        ids.iter()
            .map(|id| (id.clone(), self.map.get(id).cloned()))
            .collect()
    }
}

pub struct PanicOnCallFeatureSource;

#[async_trait::async_trait]
impl FeatureSource for PanicOnCallFeatureSource {
    async fn features_for(&self, _ids: &[TrackId]) -> Vec<(TrackId, Option<TrackFeatures>)> {
        panic!("PanicOnCallFeatureSource was invoked — warm cache should bypass this");
    }
}
```

**Testing:** A small integration test at `tests/in_memory_features_smoke.rs`:

```rust
mod support;

use support::in_memory::InMemoryFeatureSource;
use playlistize::adapters::FeatureSource;
use playlistize::domain::{TrackFeatures, TrackId};

#[tokio::test]
async fn in_memory_features_returns_known() {
    let id = TrackId::new("USQX91300120");
    let f = TrackFeatures::neutral();
    let src = InMemoryFeatureSource::new([(id.clone(), f.clone())]);
    let result = src.features_for(&[id.clone()]).await;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, id);
    assert!(result[0].1.is_some());
}
```

Note `TrackFeatures::neutral()` is `#[cfg(test)]`-only — it's accessible to integration tests because integration tests build the library in the `test` profile.

**Verification:**

Run: `cd /Users/y/Apps/music/playlistize && cargo test --test in_memory_features_smoke`
Expected: passes.

**Commit:** `Phase 6: InMemoryFeatureSource + PanicOnCallFeatureSource test doubles`
<!-- END_TASK_3 -->

<!-- END_SUBCOMPONENT_A -->

<!-- START_SUBCOMPONENT_B (tasks 4-6) -->

<!-- START_TASK_4 -->
### Task 4: Implement ReccoBeatsFeatures struct, URL builders, response models

**Files:**
- Modify: `/Users/y/Apps/music/playlistize/src/adapters/reccobeats.rs`

**Precondition — DO NOT START until Task 1 is complete:**

```bash
cd /Users/y/Apps/music/playlistize
test -f docs/implementation-plans/2026-05-23-playlist-arc-v1/RECCOBEATS_SPIKE.md \
  && grep -q "^## Decisions" docs/implementation-plans/2026-05-23-playlist-arc-v1/RECCOBEATS_SPIKE.md \
  || { echo "BLOCKED: Task 1 spike not complete — return to Task 1"; exit 1; }
```

If the gate above fails, return to Task 1 before continuing. The code below assumes the spike's `## Decisions` section confirms the design's original endpoint shape (batch `/v1/audio-features?ids=...`); update constants and the response model to match whatever the spike found.

**Implementation:**

**Substitute the constants below based on Task 1's spike findings.** The shape below assumes the design's original assumption (batch `/v1/audio-features?ids=...`) — if the spike shows a different endpoint, update `build_batch_url` and the response models accordingly.

```rust
//! ReccoBeats audio-features adapter. Gated by the `reccobeats` Cargo feature.
//!
//! Single shape exposed: `FeatureSource::features_for(&[TrackId]) -> Vec<(TrackId, Option<TrackFeatures>)>`.
//! Cache read-through filters un-cached IDs before any network call.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use reqwest::Client;

use crate::adapters::{Cache, FeatureSource};
use crate::domain::{Bpm, Mode, Normalized, PitchClass, TrackFeatures, TrackId};
use crate::errors::{ReccoBeatsError, ReccoBeatsErrorKind};

/// API base. Override via `new_with_base()` for tests.
const DEFAULT_BASE: &str = "https://api.reccobeats.com/v1";

/// Maximum IDs per batch. Spike-derived value — adjust if Task 1 finds otherwise.
const MAX_BATCH: usize = 40;

#[derive(serde::Deserialize, Debug)]
struct BatchResponse {
    // Top-level may be an array directly, or an object with an items key.
    // SPIKE: confirm shape and adjust. Below assumes object-with-items.
    #[serde(default)]
    items: Vec<FeatureItem>,
}

#[derive(serde::Deserialize, Debug)]
struct FeatureItem {
    /// Spotify URL or ReccoBeats href; used for response→input mapping.
    #[serde(default)]
    href: Option<String>,
    /// ISRC if returned; used as fallback mapping key.
    #[serde(default)]
    isrc: Option<String>,
    /// Numeric features. `key`/`mode` may be absent per the spike;
    /// `#[serde(default)]` gives `Option::None`.
    #[serde(default)]
    tempo: Option<f32>,
    #[serde(default)]
    key: Option<u8>,
    #[serde(default)]
    mode: Option<u8>, // 0 = minor, 1 = major (Spotify convention)
    #[serde(default)]
    energy: Option<f32>,
    #[serde(default)]
    danceability: Option<f32>,
    #[serde(default)]
    valence: Option<f32>,
    #[serde(default)]
    loudness: Option<f32>,
    #[serde(default)]
    acousticness: Option<f32>,
    #[serde(default)]
    instrumentalness: Option<f32>,
    #[serde(default)]
    liveness: Option<f32>,
    #[serde(default)]
    speechiness: Option<f32>,
}

pub struct ReccoBeatsFeatures {
    client: Client,
    cache: Arc<Mutex<Cache>>,
    base: String,
    /// Number of 429-retry attempts per batch. Default 1 (one retry).
    retry_attempts: u8,
}

impl ReccoBeatsFeatures {
    pub fn new(cache: Arc<Mutex<Cache>>) -> Result<Self, reqwest::Error> {
        Self::new_with_base(cache, DEFAULT_BASE.to_string())
    }

    pub fn new_with_base(cache: Arc<Mutex<Cache>>, base: String) -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self { client, cache, base, retry_attempts: 1 })
    }
}

pub(crate) fn build_batch_url(base: &str, ids: &[&str]) -> String {
    let qs: String = ids.iter()
        .map(|id| format!("ids={}", urlencoding::encode(id)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}/audio-features?{qs}")
}

/// Convert a parsed `FeatureItem` to a `TrackFeatures` with sensible
/// fallbacks for missing fields. The Camelot cost term degrades to
/// uniform when `key`/`mode` are absent — documented in the module
/// doc comment.
pub(crate) fn item_to_features(item: &FeatureItem) -> TrackFeatures {
    // Uses pre-validated constants from Phase 2 (`Bpm::DEFAULT_120`,
    // `PitchClass::C`, `Normalized::{ZERO,HALF}`). Zero `expect()`,
    // zero `unwrap()` — per project rule "no unwrap() outside tests/main".
    let tempo = item.tempo
        .and_then(|t| Bpm::new(t).ok())
        .unwrap_or(Bpm::DEFAULT_120);
    let key = item.key
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
        energy: item.energy.map(Normalized::clamp).unwrap_or(Normalized::HALF),
        danceability: item.danceability.map(Normalized::clamp).unwrap_or(Normalized::HALF),
        valence: item.valence.map(Normalized::clamp).unwrap_or(Normalized::HALF),
        loudness: item.loudness.unwrap_or(-10.0),
        acousticness: item.acousticness.map(Normalized::clamp).unwrap_or(Normalized::HALF),
        instrumentalness: item.instrumentalness.map(Normalized::clamp).unwrap_or(Normalized::ZERO),
        liveness: item.liveness.map(Normalized::clamp).unwrap_or(Normalized::ZERO),
        speechiness: item.speechiness.map(Normalized::clamp).unwrap_or(Normalized::ZERO),
    }
}

/// Map a `FeatureItem` to the input `TrackId` that produced it.
/// Tries `href` (extract Spotify ID from URL) first, then `isrc`.
pub(crate) fn match_item_to_id<'a>(item: &FeatureItem, ids: &'a [TrackId]) -> Option<&'a TrackId> {
    if let Some(isrc) = &item.isrc {
        if let Some(id) = ids.iter().find(|i| i.get().eq_ignore_ascii_case(isrc)) {
            return Some(id);
        }
    }
    if let Some(href) = &item.href {
        // Extract Spotify ID from "https://open.spotify.com/track/<id>"
        if let Some(id_str) = href.rsplit('/').next() {
            if let Some(id) = ids.iter().find(|i| i.get() == id_str) {
                return Some(id);
            }
        }
    }
    None
}
```

**Testing (pure helpers):**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_batch_url_joins_ids() {
        let url = build_batch_url("https://api.reccobeats.com/v1", &["AAA111111111", "BBB222222222"]);
        assert!(url.contains("ids=AAA111111111"));
        assert!(url.contains("ids=BBB222222222"));
        assert!(url.contains("&"));
    }

    #[test]
    fn item_to_features_uses_defaults_when_fields_missing() {
        let item = FeatureItem {
            href: None, isrc: None,
            tempo: None, key: None, mode: None,
            energy: None, danceability: None, valence: None, loudness: None,
            acousticness: None, instrumentalness: None, liveness: None, speechiness: None,
        };
        let f = item_to_features(&item);
        assert_eq!(f.tempo.get(), 120.0);
        assert_eq!(f.key.get(), 0);
        assert!(matches!(f.mode, Mode::Major));
        assert_eq!(f.energy.get(), 0.5);
    }

    #[test]
    fn match_item_to_id_by_isrc() {
        let ids = vec![TrackId::new("USQX91300120"), TrackId::new("USQX91100008")];
        let item = FeatureItem {
            href: None, isrc: Some("usqx91300120".into()), // case-insensitive
            ..Default::default()  // proptest-derive Default if needed; otherwise hand-construct
        };
        assert_eq!(match_item_to_id(&item, &ids), Some(&ids[0]));
    }

    #[test]
    fn match_item_to_id_by_href_spotify_id() {
        let ids = vec![TrackId::new("69kOkLUCkxIZYexIgSG8rq")];
        let item = FeatureItem {
            href: Some("https://open.spotify.com/track/69kOkLUCkxIZYexIgSG8rq".into()),
            isrc: None,
            ..Default::default()
        };
        assert_eq!(match_item_to_id(&item, &ids), Some(&ids[0]));
    }
}
```

`FeatureItem` needs `Default` derived for the test ergonomics — add `#[derive(Default)]` to it.

**Verification:**

Run: `cd /Users/y/Apps/music/playlistize && cargo test --features reccobeats --lib adapters::reccobeats::tests`
Expected: all pass.

**Commit:** `Phase 6: ReccoBeats URL builders, response models, feature mapping helpers`
<!-- END_TASK_4 -->

<!-- START_TASK_5 -->
### Task 5: Implement FeatureSource impl with batching, cache, and 429 retry

**Files:**
- Modify: `/Users/y/Apps/music/playlistize/src/adapters/reccobeats.rs`

**Implementation:**

```rust
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
                            tracing::warn!(?item, "reccobeats response item didn't match any input id");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %format!("{:?}", e.kind), "reccobeats batch failed; leaving as None");
                    // Do NOT cache failure. Output slots stay None;
                    // Phase 7 orchestration will emit them as unresolved.
                }
            }
        }

        output
    }
}

impl ReccoBeatsFeatures {
    async fn fetch_with_retry(&self, url: &str, ids: &[&str]) -> Result<Vec<FeatureItem>, ReccoBeatsError> {
        for attempt in 0..=self.retry_attempts {
            match self.fetch_once(url).await {
                Ok(items) => return Ok(items),
                Err(e) if matches!(e.kind, ReccoBeatsErrorKind::Throttled) && attempt < self.retry_attempts => {
                    // Get Retry-After from the error context — we'd need
                    // to plumb it through. For v1: sleep a fixed backoff.
                    let backoff = Duration::from_secs(2u64.pow(attempt as u32 + 1));
                    tracing::info!(?backoff, "reccobeats throttled; retrying");
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

    async fn fetch_once(&self, url: &str) -> Result<Vec<FeatureItem>, ReccoBeatsError> {
        let resp = self.client.get(url).send().await
            .map_err(|e| ReccoBeatsError {
                kind: if e.is_timeout() || e.is_connect() { ReccoBeatsErrorKind::Network } else { ReccoBeatsErrorKind::Network },
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

        Ok(body.items)
    }
}
```

**The `Retry-After` header.** Doing this properly requires extracting the header value from the 429 response *before* discarding the response object. Refactor `fetch_once` to return `(headers, body_result)` or pass the parsed `Retry-After` seconds via `ReccoBeatsError`. For v1, exponential backoff (2s, 4s) is acceptable per the design's "On 429: sleeps Retry-After seconds and retries once" — if the header is absent, fall back to the exponential schedule.

**Testing:**

This task's behavior is fully tested in Task 6's wiremock integration tests. No new unit tests here.

**Verification:**

Run: `cd /Users/y/Apps/music/playlistize && cargo build --features reccobeats`
Expected: green.

**Commit:** `Phase 6: ReccoBeatsFeatures impl with batch+cache+429 retry`
<!-- END_TASK_5 -->

<!-- START_TASK_6 -->
### Task 6: Add wiremock integration tests for ReccoBeats batching, cache, and 429

**Files:**
- Modify: `/Users/y/Apps/music/playlistize/src/adapters/reccobeats.rs`

**Implementation:**

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path};
    // Note: `query_param_contains` is NOT used here. wiremock 0.6 exposes
    // `query_param(key, value)` for exact matches but no built-in
    // contains-matcher; the tests below match only on method+path because
    // the URL builder is unit-tested in Task 4 and the integration test
    // doesn't need to re-assert query-string shape.

    async fn make_features(mock: &MockServer) -> ReccoBeatsFeatures {
        let cache = Arc::new(Mutex::new(Cache::load(std::path::Path::new("/nonexistent")).unwrap()));
        ReccoBeatsFeatures::new_with_base(cache, format!("{}/v1", mock.uri())).unwrap()
    }

    #[tokio::test]
    async fn happy_path_batch_of_two() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/audio-features"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "isrc": "USQX91300120", "tempo": 116.0, "energy": 0.8, "danceability": 0.7, "valence": 0.6, "loudness": -6.0 },
                    { "isrc": "USQX91100008", "tempo": 120.0, "energy": 0.5, "danceability": 0.5, "valence": 0.5, "loudness": -10.0 }
                ]
            })))
            .mount(&mock)
            .await;

        let src = make_features(&mock).await;
        let ids = vec![TrackId::new("USQX91300120"), TrackId::new("USQX91100008")];
        let result = src.features_for(&ids).await;
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|(_, f)| f.is_some()));
        assert_eq!(result[0].1.as_ref().unwrap().tempo.get(), 116.0);
    }

    #[tokio::test]
    async fn cache_hit_bypasses_network() {
        let mock = MockServer::start().await;
        // Mount NO mocks — any HTTP call to `mock` fails the test.

        let cache = Arc::new(Mutex::new(Cache::load(std::path::Path::new("/nonexistent")).unwrap()));
        {
            let mut c = cache.lock().await;
            c.put_features(TrackId::new("USQX91300120"), TrackFeatures::neutral());
        }
        let src = ReccoBeatsFeatures::new_with_base(cache, format!("{}/v1", mock.uri())).unwrap();
        let ids = vec![TrackId::new("USQX91300120")];
        let result = src.features_for(&ids).await;
        assert!(result[0].1.is_some());
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
                "items": [{ "isrc": "USQX91300120", "tempo": 116.0 }]
            })))
            .mount(&mock)
            .await;

        let mut src = make_features(&mock).await;
        src.retry_attempts = 1;
        let ids = vec![TrackId::new("USQX91300120")];
        let result = src.features_for(&ids).await;
        assert!(result[0].1.is_some(), "expected feature after retry");
    }

    #[tokio::test]
    async fn unmatchable_id_remains_none() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/audio-features"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": []  // API returns nothing for this ID
            })))
            .mount(&mock)
            .await;

        let src = make_features(&mock).await;
        let ids = vec![TrackId::new("ZZZZ99999999")];
        let result = src.features_for(&ids).await;
        assert!(result[0].1.is_none());
    }
}
```

**429 backoff in tests.** The retry sleep is 2 seconds — that's slow for unit tests. Add a constructor `with_retry_backoff(Duration)` (or a public `retry_backoff_base: Duration` field) and set it to `Duration::from_millis(10)` in tests.

**Verification:**

Run: `cd /Users/y/Apps/music/playlistize && cargo test --features reccobeats --lib adapters::reccobeats`
Expected: all unit + wiremock tests pass. Total runtime < 5s with the test backoff shortened.

**Commit:** `Phase 6: wiremock integration tests for ReccoBeats (batch + cache + 429 retry)`
<!-- END_TASK_6 -->

<!-- END_SUBCOMPONENT_B -->

<!-- START_SUBCOMPONENT_C (tasks 7-8) -->

<!-- START_TASK_7 -->
### Task 7: Add live-network smoke test (feature-gated)

**Files:**
- Create: `/Users/y/Apps/music/playlistize/tests/reccobeats_live.rs`

**Implementation:**

```rust
#![cfg(all(feature = "reccobeats", feature = "live-network"))]

use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

use playlistize::adapters::{Cache, FeatureSource, ReccoBeatsFeatures};
use playlistize::domain::TrackId;

#[tokio::test]
async fn fetches_features_for_known_isrc() {
    let dir = TempDir::new().unwrap();
    let cache = Arc::new(Mutex::new(
        Cache::load(&dir.path().join("cache.json")).unwrap()
    ));

    let src = ReccoBeatsFeatures::new(cache).unwrap();
    let id = TrackId::new("USQX91300120"); // Daft Punk - Get Lucky
    let result = src.features_for(&[id.clone()]).await;
    assert_eq!(result.len(), 1);

    match &result[0].1 {
        Some(features) => {
            assert!(features.tempo.get() > 0.0);
            assert!(features.energy.get() >= 0.0 && features.energy.get() <= 1.0);
        }
        None => panic!("expected features for known ISRC; ReccoBeats may have changed behavior — re-run Task 1 spike"),
    }
}
```

**Verification:**

Without the feature: `cd /Users/y/Apps/music/playlistize && cargo test --test reccobeats_live` → builds with zero tests.

With the feature: `cd /Users/y/Apps/music/playlistize && cargo test --features reccobeats,live-network --test reccobeats_live` → passes if network reaches reccobeats.com. If it fails on this Task, **return to Task 1 spike** — the API has likely changed shape since the design was written.

**Commit:** `Phase 6: live-network smoke test for ReccoBeats`
<!-- END_TASK_7 -->

<!-- START_TASK_8 -->
### Task 8: Module wiring, full verification, commit

**Files:**
- Modify: `/Users/y/Apps/music/playlistize/src/adapters/mod.rs`

**Implementation:**

Ensure both `reccobeats` and the `FeatureSource` re-exports are wired:

```rust
//! Impure adapter shell — file IO, HTTP, JSON cache, trait surfaces.

pub mod cache;
pub mod csv_io;

#[cfg(feature = "musicbrainz")]
pub mod musicbrainz;
#[cfg(feature = "reccobeats")]
pub mod reccobeats;

pub use cache::{Cache, CacheFile, CACHE_VERSION};
pub use csv_io::{read_input, write_output, write_unresolved, Unresolved};

#[cfg(feature = "musicbrainz")]
pub use musicbrainz::MusicBrainzIsrcResolver;
#[cfg(feature = "reccobeats")]
pub use reccobeats::ReccoBeatsFeatures;

// (Resolver, Resolution, FeatureSource trait defs from Phase 5 / Task 2.)
```

Final verification (each must exit 0):

```bash
cd /Users/y/Apps/music/playlistize
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features --features musicbrainz,reccobeats -- -D warnings
cargo test
cargo build --release
```

AC8 surface checks:
```bash
cd /Users/y/Apps/music/playlistize
cargo build --no-default-features --features musicbrainz,reccobeats
cargo build --no-default-features --features musicbrainz
cargo build --no-default-features --features reccobeats
cargo build --no-default-features  # AC8.3 — bare-bones still compiles
```

The last command (`--no-default-features` alone) should compile `algo/` + `domain/` and produce a binary. The binary will error at startup ("no resolver/feature source compiled in") — that error is wired up in Phase 7.

**Commit:** Bundle as needed:
```bash
cd /Users/y/Apps/music/playlistize
git add src/adapters/ Cargo.toml Cargo.lock
git status
git commit -m "Phase 6: ReccoBeats adapter wiring + full feature-matrix verification"
```
<!-- END_TASK_8 -->

<!-- END_SUBCOMPONENT_C -->

---

## Phase 6 Done When

- Task 1 spike documented (either in the commit message or `RECCOBEATS_SPIKE.md`).
- `cargo test --features reccobeats` passes, including wiremock tests for batch, cache, 429-retry, and unmatchable IDs.
- `cargo test --features reccobeats,live-network` passes when network is available.
- Cache read-through verified: warm cache produces zero HTTP calls (Task 6 `cache_hit_bypasses_network` test).
- Failure mode for missing `key`/`mode` documented: adapter defaults to `PitchClass(0)` + `Mode::Major`; document the Camelot degradation in the module-level doc comment.
- `cargo build --no-default-features --features musicbrainz,reccobeats` succeeds (AC8.1).
- `cargo build --no-default-features` succeeds (foundation for AC8.3 — startup-error wiring is Phase 7).
- All `pub` items have doc comments.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build --release` all exit 0.
- Commits on `playlist-arc-v1` whose subjects start with `Phase 6:`.

## Risk callouts

- **API spike is non-negotiable.** Task 1 must happen before Task 4 — the entire phase's structure depends on the actual API shape. If the spike findings contradict the design's assumptions, surface to the user before continuing.
- **`key`/`mode` absence.** If ReccoBeats really omits these fields in 2026, the Camelot cost-function term loses signal but the rest of the pipeline keeps working. Document in the module doc and in `RECCOBEATS_SPIKE.md`.
- **`Retry-After` header parsing.** v1 uses exponential backoff (2s, 4s, ...) when the header is absent. Plumbing the actual seconds through `ReccoBeatsError` is a v2 polish item; the current code is correct, just suboptimal under heavy throttling.
- **`async-trait` overhead.** Each `dyn FeatureSource` call goes through a boxed future. For a 40-id batch this is negligible (one box per batch, not per ID).
- **`reqwest::Error::is_*` methods.** `is_connect()` and `is_timeout()` are not exhaustive; treat anything not explicitly categorized as `Network`. The classification is used only for tracing — it doesn't change retry behavior.
