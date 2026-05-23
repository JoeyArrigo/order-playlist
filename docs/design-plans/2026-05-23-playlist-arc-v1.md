# Playlist Arc Ordering — v1 Design

## Summary

Playlist Arc v1 is a Rust command-line tool that takes a `title,artist` CSV and reorders it so the playlist follows a target energy arc — a fixed asymmetric curve that peaks roughly two-thirds of the way through. The tool resolves each track's audio features through a two-stage adapter chain: MusicBrainz converts a `(title, artist)` pair into an ISRC, then ReccoBeats converts that ISRC into numeric features (energy, tempo, key, danceability, valence, etc.). Results are cached in a JSON sidecar so repeat runs hit zero network. The final reordering is produced by a simulated-annealing optimizer running over a weighted cost function.

The architecture is organized as a functional core / imperative shell: `domain/` and `algo/` are pure synchronous Rust with no IO, while `adapters/` and `cli/` hold all network calls, file reads, and terminal output. Adapter implementations are swapped in or out at compile time using Cargo feature flags, so new data providers (e.g., Spotify) can be added without touching the algorithm. A seeded ChaCha20 RNG makes output byte-identical across runs when input, cache, and seed are held constant.

## Definition of Done

1. Given `data/input.csv` with `title,artist` columns, produce `data/output.csv` with the same songs reordered to optimize the cost function, augmented with resolved audio features.
2. Reordered playlist visibly follows a target energy arc; an ASCII chart of the energy curve is printed to stdout.
3. No two songs share an artist within 4 positions (configurable via `--artist-window`, default 4).
4. Tracks where audio-feature lookup fails are logged with title+artist, skipped without crashing, and written to an `unresolved.csv` sidecar in the same format as the input (so the user can correct and retry).
5. A `--seed` flag produces byte-identical output across runs given the same inputs and cache state.
6. A summary report (resolved/unresolved counts, before/after total cost, before/after arc deviation) is emitted to stdout at the end of the run.
7. Resolved audio features are cached in a sidecar JSON next to the input CSV; re-runs hit zero network when the cache is warm.
8. Primary audio-feature source is ReccoBeats; the resolver and feature source are selectable at compile time via Cargo features, and the architecture allows additional providers to be added without core algorithm changes.

**Explicit non-goals for v1:** runtime/latency targets, anchor songs, banger pacing, Apple Music MCP integration, local audio analysis, web UI, ML / embedding-based recommendation, real-time crowd-response reordering.

## Acceptance Criteria

### playlist-arc-v1.AC1: Input CSV is read, reordered, augmented with features, and written
- **playlist-arc-v1.AC1.1 Success:** Input CSV with `title,artist` header and N rows produces output CSV with the same N tracks reordered, with feature columns (`position, tempo, key, mode, energy, danceability, valence, loudness, isrc`) appended.
- **playlist-arc-v1.AC1.2 Success:** Extra columns in input CSV are tolerated (ignored, not preserved in output).
- **playlist-arc-v1.AC1.3 Failure:** Input path does not exist → exit 3 with `InputError::NotFound`, miette message identifying the path.
- **playlist-arc-v1.AC1.4 Failure:** Missing `title` or `artist` header → exit 3 with `InputError::Csv` pointing at the header row.
- **playlist-arc-v1.AC1.5 Failure:** Header-only / zero-row input → exit 3 with a clear "no tracks" message; no silent empty output.
- **playlist-arc-v1.AC1.6 Edge:** Output path's parent directory does not exist → exit 3 with a clear message; the binary does not create directories.

### playlist-arc-v1.AC2: ASCII energy-arc chart is printed to stdout
- **playlist-arc-v1.AC2.1 Success:** A chart of per-position energy is printed after the reordering.
- **playlist-arc-v1.AC2.2 Success:** Chart rendering is deterministic — snapshot-tested via `insta`.
- **playlist-arc-v1.AC2.3 Edge:** Renders without crashing for very small N (1, 2, 3 tracks).

### playlist-arc-v1.AC3: No two songs share an artist within `--artist-window` positions
- **playlist-arc-v1.AC3.1 Success:** Default window=4: no two tracks within 4 positions of each other have matching `artist` (case-insensitive exact match) in any output.
- **playlist-arc-v1.AC3.2 Success:** `--artist-window=0` disables the constraint (adjacent same-artist permitted).
- **playlist-arc-v1.AC3.3 Success:** Arbitrary positive N is honored.
- **playlist-arc-v1.AC3.4 Edge:** Infeasible input (e.g., all tracks by one artist with window=4) still completes; summary report exposes remaining clashes.
- **playlist-arc-v1.AC3.5 Verification:** Property test asserts the invariant over random inputs with default window.

### playlist-arc-v1.AC4: Unresolved tracks are logged and written to a sidecar CSV
- **playlist-arc-v1.AC4.1 Success:** Unresolvable queries are omitted from the optimizer input and written to `unresolved.csv` with `title,artist,reason` columns.
- **playlist-arc-v1.AC4.2 Success:** Unresolved tracks do not cause non-zero exit if at least one track resolved.
- **playlist-arc-v1.AC4.3 Success:** Each unresolved track is logged at `WARN` via `tracing`.
- **playlist-arc-v1.AC4.4 Failure:** When *all* tracks fail to resolve → exit 5 with a "nothing to anneal" message.
- **playlist-arc-v1.AC4.5 Edge:** Sidecar rows are re-feedable (compatible with `read_input`).

### playlist-arc-v1.AC5: Deterministic runs with `--seed`
- **playlist-arc-v1.AC5.1 Success:** Two consecutive runs with identical input + cache + seed produce byte-identical `output.csv`.
- **playlist-arc-v1.AC5.2 Success:** Same for `unresolved.csv`.
- **playlist-arc-v1.AC5.3 Success:** Different seeds with the same input produce different orderings (non-trivial search space).
- **playlist-arc-v1.AC5.4 Edge:** Omitting `--seed` derives a seed from system time and logs the chosen seed at `INFO` for reproducibility.

### playlist-arc-v1.AC6: Summary report is emitted to stdout
- **playlist-arc-v1.AC6.1 Success:** Report includes: resolved count, unresolved count, before/after total cost, before/after arc deviation, per-cost-term breakdown.
- **playlist-arc-v1.AC6.2 Success:** Snapshot-tested via `insta`.
- **playlist-arc-v1.AC6.3 Edge:** When unresolved count > 0, report refers the user to the sidecar.

### playlist-arc-v1.AC7: Cache hits zero network on warm runs
- **playlist-arc-v1.AC7.1 Success:** With a fully warm cache, integration test `tests/zero_network.rs` with `PanicOnCallResolver` + `PanicOnCallFeatureSource` completes without panicking.
- **playlist-arc-v1.AC7.2 Success:** Cache is written atomically (temp + rename); a mid-run SIGKILL leaves the previous cache intact.
- **playlist-arc-v1.AC7.3 Failure:** Cache `version` mismatch → exit 4 with `CacheError::VersionMismatch` and a "delete or upgrade" hint.
- **playlist-arc-v1.AC7.4 Edge:** Corrupt JSON → exit 4 with `CacheError::Corrupt` and the underlying serde error attached.

### playlist-arc-v1.AC8: Adapters are swappable via Cargo features
- **playlist-arc-v1.AC8.1 Success:** `cargo build --no-default-features --features musicbrainz,reccobeats` succeeds and is behaviorally bit-equivalent to the default build.
- **playlist-arc-v1.AC8.2 Success:** A future feature (e.g. `spotify`) can introduce a new `Resolver` impl with zero changes to `algo/`, `domain/`, or `cli/`.
- **playlist-arc-v1.AC8.3 Edge:** `--no-default-features` with no provider features compiles `algo/` + `domain/`; the resulting binary errors clearly at startup ("no resolver/feature source compiled in").

### playlist-arc-v1.AC9: Cross-cutting
- **playlist-arc-v1.AC9.1:** Default `cargo test` issues zero network requests; live tests only run with `--features live-network`; no silent skips.
- **playlist-arc-v1.AC9.2:** Adapter modules emit `tracing::info!` on every network call with structured fields (title, artist, id).
- **playlist-arc-v1.AC9.3:** `main.rs` installs a `miette` panic hook; user-facing errors include source spans and help text.

## Glossary

- **2-swap**: A perturbation step in the annealing loop that swaps two randomly chosen tracks in the ordering; the simplest neighborhood move for permutation optimization.
- **`algo/`**: The pure algorithm crate sub-directory; contains cost functions, the energy arc, and the annealer — no IO allowed.
- **Annealing / simulated annealing**: A probabilistic optimization algorithm that iteratively swaps elements, occasionally accepting worse solutions to escape local optima, then gradually reduces this tolerance ("cools").
- **Arc / energy arc**: The target per-position energy curve the playlist is shaped to follow; in this design a fixed asymmetric beta-like shape peaking near position 0.68.
- **Arc deviation**: A scalar measure of how far the actual per-position energy values are from the target arc curve.
- **Artist window (`--artist-window`)**: A constraint preventing two tracks by the same artist from appearing within N positions of each other in the output.
- **`cargo build --no-default-features --features ...`**: A Cargo (Rust build tool) invocation that disables all default optional dependencies and enables only explicitly named ones.
- **Cargo features**: Rust's compile-time conditional compilation system; used here to select which adapter implementations are compiled into the binary.
- **Camelot wheel / CamelotCode**: A DJ harmonic-mixing notation system that maps musical keys to a 24-position wheel (12 major + 12 minor); adjacent positions are harmonically compatible.
- **`ChaCha20Rng`**: A cryptographically strong, deterministic pseudo-random number generator from the `rand_chacha` crate; guarantees identical output for a given seed.
- **`clap`**: A Rust library for parsing command-line arguments using derive macros.
- **`CostContext` / `CostWeights`**: Structs carrying the weighting parameters fed to the cost function; control how much each penalty term (arc deviation, Camelot clash, artist spacing) contributes to total cost.
- **`delta_cost`**: An incremental cost calculation that updates only the terms affected by a single 2-swap, avoiding a full re-evaluation of the entire ordering — critical for annealing performance.
- **`domain/`**: The pure types sub-directory; defines newtypes like `Bpm`, `PitchClass`, `Normalized`, and `TrackFeatures` with validation baked into constructors.
- **FCIS (Functional Core, Imperative Shell)**: An architectural pattern where pure business logic lives in an inner core with no IO, and all side effects (network, disk) are confined to an outer shell.
- **Geometric cooling**: A simulated-annealing schedule where temperature is multiplied by a constant factor α (< 1) each iteration, producing exponential decay.
- **`insta`**: A Rust snapshot-testing library; saves expected output to versioned files and diffs against them on subsequent runs.
- **ISRC (International Standard Recording Code)**: A globally unique 12-character identifier for a specific sound recording; used here as the stable ID linking MusicBrainz to ReccoBeats.
- **MBID (MusicBrainz Identifier)**: A UUID assigned to entities (recordings, releases, artists) in the MusicBrainz database; an intermediate step before obtaining an ISRC.
- **`miette`**: A Rust error-reporting library that adds source spans, help text, and human-friendly formatting to errors — used for all user-facing diagnostics.
- **MusicBrainz**: An open-source music metadata database and API; used in v1 as the resolver that maps `(title, artist)` → ISRC.
- **`Normalized`**: A newtype wrapping `f32` guaranteed to be in `[0.0, 1.0]`; used for energy, danceability, valence, etc.
- **`PanicOnCallResolver` / `PanicOnCallFeatureSource`**: Test doubles that panic if ever invoked; used to prove the warm-cache path issues zero real network calls.
- **Pilot-calibrated T₀**: An initial simulated-annealing temperature derived by running short exploratory trials to find a starting temperature that produces a meaningful acceptance rate — rather than a hand-tuned constant.
- **`proptest`**: A Rust property-based testing library; generates random inputs and asserts invariants hold for all of them.
- **ReccoBeats**: A third-party audio-feature API (no auth required in v1) that accepts ISRCs and returns numeric features like energy, tempo, and key.
- **`Resolver` / `FeatureSource`**: The two adapter traits defining how providers are swapped; `Resolver` maps `(title, artist)` → ID; `FeatureSource` maps IDs → audio features.
- **`Retry-After`**: An HTTP response header indicating how many seconds a client should wait before retrying after receiving a 429 (Too Many Requests) response.
- **Sidecar JSON / sidecar CSV**: A secondary file placed next to the primary input or output, used here for the feature cache (`.cache.json`) and unresolved tracks (`unresolved.csv`).
- **`thiserror`**: A Rust derive macro library for defining structured error types; used in library code as opposed to `anyhow` which is used only at the binary boundary.
- **`tokio`**: An asynchronous runtime for Rust; async is confined to adapter modules in this design.
- **`tracing`**: A Rust structured-logging and instrumentation library; used for `INFO`/`WARN` diagnostic events throughout the pipeline.
- **`unresolved.csv`**: The output sidecar listing tracks whose audio features could not be fetched, with a `reason` column, in a format compatible with re-feeding to the tool.

## Architecture

The crate is a single binary that pulls a title+artist CSV through a two-stage adapter chain to obtain audio features, then runs a pure simulated-annealing optimizer over a weighted cost function and emits a reordered CSV plus diagnostics.

**Functional core, imperative shell.** `domain/` and `algo/` hold pure, synchronous, IO-free code. `adapters/` and `cli/` hold all IO. The annealer takes borrows of `&[Track]` and never sees `Option<Features>` — partial-data handling is collapsed at the adapter boundary.

**Two adapter traits, each Cargo-feature-selectable.**

```rust
// adapters/mod.rs
pub trait Resolver {
    async fn resolve_many(&self, queries: &[TrackQuery]) -> Vec<Resolution>;
}
pub enum Resolution { Resolved { query: TrackQuery, id: TrackId }, Unresolved { query: TrackQuery, reason: String } }

pub trait FeatureSource {
    async fn features_for(&self, ids: &[TrackId]) -> Vec<(TrackId, Option<TrackFeatures>)>;
}
```

Default v1 implementations: `MusicBrainzIsrcResolver` (no auth; client-side studio filter on `disambiguation`; 1 req/sec pacing) and `ReccoBeatsFeatures` (no auth; batches of ≤40 IDs per call; 429 + `Retry-After` handled). The trait surface is opaque to `algo/*`, which only sees fully resolved `Track` values.

**Three-stage pipeline:** read CSV → resolve (resolver + ReccoBeats), per-stage cached in one JSON sidecar with two named maps and a `version` field → anneal → write outputs (reordered CSV, ASCII chart on stdout, summary report on stdout, unresolved sidecar CSV).

**Module layout:**

```
src/
├── main.rs                 # CLI parsing, orchestration, exit codes; only place that uses anyhow + miette::Report
├── lib.rs                  # re-exports for integration tests
├── errors.rs               # thiserror + miette::Diagnostic types: InputError, AdapterError, CacheError
├── domain/                 # pure types, zero IO
│   ├── mod.rs              # re-exports only
│   ├── track.rs            # TrackQuery, TrackId(String), Track, TrackFeatures
│   ├── newtypes.rs         # Bpm, PitchClass, Mode, Normalized
│   └── camelot.rs          # CamelotCode, CamelotLetter, (PitchClass, Mode) → CamelotCode
├── algo/                   # pure algorithm, zero IO
│   ├── mod.rs              # re-exports only
│   ├── camelot.rs          # CamelotTable (24×24 distance lookup)
│   ├── arc.rs              # EnergyArc with fixed default curve, target() + deviation_cost()
│   ├── cost.rs             # CostWeights, CostContext, total_cost, delta_cost
│   └── anneal.rs           # AnnealConfig, optimize() with seeded RNG, geometric cooling, 2-swap
├── adapters/               # impure IO
│   ├── mod.rs              # Resolver and FeatureSource trait definitions; re-exports of feature-gated impls
│   ├── musicbrainz.rs      # default Resolver impl, gated by `musicbrainz` Cargo feature
│   ├── reccobeats.rs       # default FeatureSource impl, gated by `reccobeats` Cargo feature
│   ├── cache.rs            # JSON sidecar: { version: u32, resolutions: {...}, features: {...} }
│   └── csv_io.rs           # read_input, write_output, write_unresolved
└── cli/                    # CLI surface and presentation
    ├── mod.rs              # re-exports only
    ├── args.rs             # clap derive structs
    ├── chart.rs            # ASCII energy-arc chart
    └── report.rs           # summary report formatter
```

**Algorithm contract.** `algo::optimize` is infallible by construction — it takes fully validated `Track` values and a seeded `rand::Rng`, returns `Vec<usize>`. Delta-cost is mandatory: `delta_cost(swap)` mutates only the four pairwise terms and two arc terms touched by a 2-swap, and a property test asserts `total_cost(after) ≈ total_cost(before) + delta_cost(swap)` for random orderings and swaps. The artist-spacing hard constraint is encoded as a large `artist_clash` weight in `CostWeights` — SA naturally avoids these neighborhoods, no two-tier feasibility logic needed.

**Determinism.** The `--seed` flag feeds a `rand_chacha::ChaCha20Rng`. Given identical input CSV, identical cache content, and identical seed, the binary emits byte-identical `output.csv` and `unresolved.csv`. The cache is part of the determinism contract: replacing it can change the resolved features and therefore the ordering.

## Existing Patterns

This is a greenfield project. There is no prior Rust code in this workspace, so there are no in-repo patterns to follow. The design instead derives its conventions from `design-plan-guidance.md` (project-wide rails loaded by the design workflow), specifically:

- **Pure-core / side-effect-shell** — `algo/` and `domain/` are pure; `adapters/` and `cli/` hold all IO.
- **No global state** — configuration flows through explicit struct parameters.
- **`thiserror` in libraries, `anyhow` only at the binary boundary** — paired with `miette::Diagnostic` for user-facing source spans and help text per the Rust house style.
- **Cargo features select adapter implementations** — `default = ["musicbrainz", "reccobeats"]`. Adding a future provider means adding a feature flag and an impl; algorithm code is untouched.
- **Cache aggressively, sidecar JSON next to input** — re-runs hit zero network.
- **Tokio for async; async confined to adapter modules** — algo/domain stay sync.
- **Dependencies pinned exactly** — application binary, not a published library.
- **Tests: unit in same file under `#[cfg(test)] mod tests`; integration under `tests/`; `live-network` Cargo feature for tests that hit real APIs; no `#[ignore]` and no silent-skip patterns.**

The functional-core/imperative-shell split is the load-bearing pattern. Every code review of new code in this repo should check that nothing in `algo/` or `domain/` imports `std::fs`, `tokio`, `reqwest`, or any module from `adapters/`.

## Implementation Phases

<!-- START_PHASE_1 -->
### Phase 1: Project scaffolding

**Goal:** Initialize the Rust crate, lock in pinned dependencies, and produce a `cargo build` + `cargo test` baseline that runs cleanly.

**Components:**
- `Cargo.toml` — package metadata; pinned dependencies (`clap`, `tokio`, `reqwest`, `serde`, `serde_json`, `csv`, `rand`, `rand_chacha`, `thiserror`, `miette`, `tracing`, `tracing-subscriber`, `anyhow`); dev-dependencies (`proptest`, `insta`, `test-case`, `pretty_assertions`); Cargo features (`default = ["musicbrainz", "reccobeats"]`, `musicbrainz`, `reccobeats`, `live-network`).
- `.gitignore` — `/target`, `*.cache.json`, `.env`, OS junk.
- `.env.example` — placeholder for any future credentials (none for v1).
- `src/main.rs`, `src/lib.rs` — minimal entry points.
- Empty module skeletons: `src/{domain,algo,adapters,cli}/mod.rs` as pure re-export files, with `imp.rs` placeholders underneath.
- `README.md` left untouched.

**Dependencies:** None (first phase).

**Done when:** `cargo build` succeeds, `cargo build --no-default-features --features musicbrainz,reccobeats` succeeds, `cargo build --features live-network` succeeds, `cargo test` runs zero tests and exits 0, `cargo clippy --all-targets -- -D warnings` is clean.
<!-- END_PHASE_1 -->

<!-- START_PHASE_2 -->
### Phase 2: Domain types

**Goal:** Define newtypes and primitive domain values with validation baked in at construction. No algorithm yet; only data shapes.

**Components:**
- `src/domain/newtypes.rs` — `Bpm(f32)`, `PitchClass(u8)` (0..=11, fallible constructor), `Mode { Major, Minor }`, `Normalized(f32)` (clamps to 0..=1 with `tracing::warn!` on out-of-range; clamp is idempotent).
- `src/domain/track.rs` — `TrackQuery { title, artist }`, `TrackId(String)` (opaque to algo), `Track { query, id, features }`, `TrackFeatures` with all ReccoBeats fields except `time_signature`.
- `src/domain/camelot.rs` — `CamelotLetter { A, B }`, `CamelotCode { number: u8, letter: CamelotLetter }`, `From<(PitchClass, Mode)> for CamelotCode` implementing the standard pitch-class→Camelot mapping, `CamelotCode → u8 (0..=23)` index helper.
- Unit tests: `Normalized` clamp idempotence (property), 24 parameterized `(PitchClass, Mode) → CamelotCode` cases (`test-case`), `Bpm`/`PitchClass` constructor validation.

**Dependencies:** Phase 1 (project setup).

**Done when:** Tests pass; all newtypes are `pub` with constructors that reject or clamp invalid input; no `Track` field uses a bare primitive where a newtype exists.

**ACs covered:** none directly; supports all algorithm ACs.
<!-- END_PHASE_2 -->

<!-- START_PHASE_3 -->
### Phase 3: Pure algorithm core

**Goal:** Implement the cost function, energy arc, Camelot distance, and simulated-annealing loop as pure synchronous functions.

**Components:**
- `src/algo/camelot.rs` — `CamelotTable` with 24×24 `f32` distances built from the practitioner convention (0 same, 1 adjacent/relative-flip, 2 ±2 positions, 4 ≥3 steps), wrapping at 12; symmetric.
- `src/algo/arc.rs` — `EnergyArc` (zero fields; the default curve is a fixed asymmetric beta-like shape peaking near position 0.68), `target(position, n)`, `deviation_cost(position, n, actual)`.
- `src/algo/cost.rs` — `CostWeights` (with `Default::default` baked in), `CostContext<'a>`, `total_cost`, `delta_cost`. Artist clash encoded as a high-weight pairwise term over a window.
- `src/algo/anneal.rs` — `AnnealConfig` (`Default` = α 0.998, geometric cooling, pilot-calibrated T₀, 100k iterations, 2 restarts), `optimize<R: Rng>(initial, ctx, config, rng) -> Vec<usize>`.
- Property tests: `total_cost(after_swap) ≈ total_cost(before) + delta_cost(swap)`; `optimize` returns a valid permutation; Camelot table is symmetric.
- Determinism test: `optimize` with a fixed `ChaCha20Rng` seed and fixed `&[Track]` produces identical output on repeated runs.

**Dependencies:** Phase 2.

**Done when:** All property tests pass; algorithm modules import nothing from `adapters/` or `std::fs` or `tokio`; no `anyhow::Result` in this module tree.

**ACs covered:** `playlist-arc-v1.AC3` (artist-spacing constraint, via cost function), `playlist-arc-v1.AC5` (`--seed` determinism, optimizer side).
<!-- END_PHASE_3 -->

<!-- START_PHASE_4 -->
### Phase 4: File IO — CSV and JSON cache

**Goal:** Read input CSVs, write output and unresolved CSVs, and persist a versioned JSON cache with atomic-write semantics.

**Components:**
- `src/adapters/csv_io.rs` — `read_input(path) -> Result<Vec<TrackQuery>, InputError>` (validates `title`, `artist` headers; surfaces line numbers in errors via miette spans), `write_output(path, ordering, &[Track])`, `write_unresolved(path, &[Unresolved])`.
- `src/adapters/cache.rs` — `CacheFile { version: u32, resolutions: HashMap<TrackQuery, TrackId>, features: HashMap<TrackId, TrackFeatures> }`, `Cache::load(path)` (returns `CacheError::VersionMismatch` on bump), `Cache::save_atomic(path)` (write-to-temp + rename), `get_resolution / put_resolution / get_features / put_features` accessors.
- Unit tests: malformed CSV produces a miette-decorated `InputError::Csv`; cache roundtrip (property: serialize → deserialize == identity); version-mismatch returns a structured error; atomic write does not corrupt an existing file on simulated interruption.

**Dependencies:** Phase 2.

**Done when:** Tests pass; cache file format documented inline (the `version: u32` field is incremented on any breaking change); errors include source paths and line numbers.

**ACs covered:** `playlist-arc-v1.AC1` (input/output shape), `playlist-arc-v1.AC4` (unresolved sidecar shape), `playlist-arc-v1.AC7` (cache shape, zero-network behavior verified in Phase 7).
<!-- END_PHASE_4 -->

<!-- START_PHASE_5 -->
### Phase 5: Resolver adapter — MusicBrainz

**Goal:** Resolve `(title, artist)` to a single ISRC by querying MusicBrainz, filtering out non-studio recordings, and looking up the chosen MBID's ISRC.

**Components:**
- `src/adapters/musicbrainz.rs` — `MusicBrainzIsrcResolver`, behind the `musicbrainz` Cargo feature. Implements `Resolver`. Uses `reqwest` with the required `User-Agent` header (`playlistize/<version> (<contact>)`). Enforces ≥1.1 s between requests via an internal `tokio::time::Interval`. For each query: Lucene search → top-N filter (skip `disambiguation` matching `live|remix|demo|instrumental|acoustic|karaoke|cover` case-insensitively) → MBID lookup with `inc=isrcs` until a non-empty ISRC is found → emit `Resolution::Resolved { id: TrackId(isrc) }`. Unresolvable queries become `Resolution::Unresolved { reason }`.
- Cache integration: read-through `Cache::get_resolution` before search; write-through `Cache::put_resolution` on success and explicit-failure (cache the "no ISRC found" decision so it's not re-attempted).
- Errors: `MusicBrainzError { kind: ErrorKind, query: TrackQuery, source: reqwest::Error }`, with kinds for network, parse, rate-limit, no-candidates.
- Unit tests: `tests/support/in_memory.rs` `InMemoryResolver` for downstream phases; recorded-response tests for the live HTTP code path (`live-network` feature) — fail with a clear "set `LIVE_NETWORK=1`" message if not enabled.

**Dependencies:** Phase 4 (cache).

**Done when:** `cargo test --features live-network` resolves a known studio recording (e.g., "Get Lucky" by Daft Punk) to an ISRC; default `cargo test` runs against `InMemoryResolver` only; rate-limiting is observable in `tracing` output.

**ACs covered:** `playlist-arc-v1.AC1` (partial: title+artist → ID resolution), `playlist-arc-v1.AC4` (unresolved logging path).
<!-- END_PHASE_5 -->

<!-- START_PHASE_6 -->
### Phase 6: Feature source adapter — ReccoBeats

**Goal:** Convert resolved IDs to `TrackFeatures` via ReccoBeats, batching ≤40 IDs per call, handling many-to-many ISRC dedupe, retrying on 429.

**Components:**
- `src/adapters/reccobeats.rs` — `ReccoBeatsFeatures`, behind the `reccobeats` Cargo feature. Implements `FeatureSource`. Chunks input IDs into 40-id batches. Issues `GET /v1/audio-features?ids=<id1>&ids=<id2>…`. Maps the response array back to input IDs by matching either `href` (Spotify URL) or `isrc`. When an ISRC yields multiple candidates, picks the first item in the response array (documented as the v1 strategy; mean/median may follow in v2). On 429: sleeps `Retry-After` seconds and retries once before surfacing `AdapterError::RateLimited`.
- Cache integration: read-through `Cache::get_features` per ID; only un-cached IDs are batched; write-through on success.
- Errors: `ReccoBeatsError { kind, ids, source }`, kinds for network, parse, throttled.
- Unit tests: in-memory `InMemoryFeatureSource` for downstream phases; live HTTP test (`live-network`) against a known ISRC.

**Dependencies:** Phase 4 (cache).

**Done when:** `cargo test --features live-network` returns features for a known ISRC; ID-to-feature mapping handles all three ID types (Spotify ID, ReccoBeats UUID, ISRC); rate-limit recovery is observable in `tracing` output.

**ACs covered:** `playlist-arc-v1.AC1` (partial: ID → features).
<!-- END_PHASE_6 -->

<!-- START_PHASE_7 -->
### Phase 7: CLI integration, presentation, and end-to-end tests

**Goal:** Wire `main.rs` to orchestrate the pipeline, parse args, render outputs, set exit codes, and verify every DoD criterion with integration tests.

**Components:**
- `src/cli/args.rs` — clap derive `Args { input: PathBuf, output: PathBuf, unresolved: PathBuf, cache: Option<PathBuf>, seed: u64, artist_window: u8, verbose: u8 }`. Defaults: `unresolved` next to `output`, `cache` next to `input`, `artist_window = 4`, `seed = system-time-derived if absent`.
- `src/cli/chart.rs` — `render_arc(&[Track], &[usize], width: usize) -> String` producing a deterministic ASCII chart of energy vs position.
- `src/cli/report.rs` — `format_summary(...)`-style formatter for resolved/unresolved counts, before/after total cost, before/after arc deviation.
- `src/main.rs` — orchestration, `tracing-subscriber` setup, miette panic hook, semantic exit codes (0 success, 2 args, 3 input, 4 cache, 5 nothing-resolved, 6 network-exhausted, 1 other).
- `tests/end_to_end.rs` — full pipeline against `tests/fixtures/small_party.csv` + pre-warmed `small_party.cache.json` using `InMemoryResolver` / `InMemoryFeatureSource`; asserts output CSV row count, presence of feature columns, exit 0.
- `tests/determinism.rs` — runs the pipeline twice in sequence; asserts byte-identical `output.csv` and `unresolved.csv`.
- `tests/unresolved.rs` — runs against `with_bad_rows.csv` (mix of resolvable and bogus); asserts unresolved sidecar content and exit 0 (not 5).
- `tests/zero_network.rs` — uses `PanicOnCallResolver` and `PanicOnCallFeatureSource` with a fully warm cache; the run must complete without panicking, proving zero-network behavior.
- `tests/cargo_features.rs` is replaced by a CI matrix entry (no Cargo-side test framework runs `cargo build` itself); CI runs `cargo build --no-default-features --features musicbrainz,reccobeats` to prove feature-swap compatibility.
- `insta` snapshots for the ASCII chart and summary report from `small_party.csv`.

**Dependencies:** Phases 3, 4, 5, 6.

**Done when:** All integration tests pass; `cargo test` is hermetic (no network); a CI workflow (or its checklist equivalent in `.github/workflows/ci.yml`) builds the Cargo-feature matrix; running `cargo run -- --input data/input.csv --output data/output.csv --seed 42` on a small hand-crafted CSV produces a sensible reordering with a visible arc and a printed summary.

**ACs covered:** `playlist-arc-v1.AC1` (full flow), `playlist-arc-v1.AC2` (chart), `playlist-arc-v1.AC4` (unresolved sidecar), `playlist-arc-v1.AC5` (determinism end-to-end), `playlist-arc-v1.AC6` (report), `playlist-arc-v1.AC7` (zero-network warm cache), `playlist-arc-v1.AC8` (Cargo-feature swap).
<!-- END_PHASE_7 -->

## Additional Considerations

**Cache as part of the determinism contract.** Two runs are byte-identical only when input CSV, cache state, *and* seed match. Deleting the cache between runs may resolve to different ISRCs (MusicBrainz search ranking can shift, ReccoBeats may surface different first-candidates), which can change the final ordering. This is acceptable for v1 and documented inline in the cache module, but the determinism test covers only the warm-cache path. A property test could later sweep cache permutations if stronger determinism becomes important.

**MusicBrainz 1 req/sec pacing.** Cold-cache first runs on an 80-track playlist take ~2–4 minutes wall-clock to resolve, dominated by MB rate-limiting. AC2 (runtime) has been removed from this design; the warm-cache path is fast because it touches zero network. If this becomes painful, future options include: an authenticated MB user agent (higher quota; needs a free MetaBrainz account), batching via MB's `browse?artist=...&inc=isrcs` endpoint to amortize artist-clustered queries, or switching to a paid resolver with higher throughput.

**ISRC many-to-many disambiguation.** ReccoBeats returns multiple feature rows for ISRCs shared across re-releases. Experiment showed audio features are near-identical across releases, with occasional tempo-detection outliers (one case had a 2× tempo error in four samples). v1 picks the first response item. If outliers cause visibly bad orderings in practice, the v2 strategy is to compute the median across response candidates (robust to the 2× outlier case). The first/median choice lives in `reccobeats.rs` and is not visible from `algo/`.

**Spotify path as a future Cargo feature.** Should ReccoBeats stop working, or should MusicBrainz become too painful, a `SpotifySearchResolver` could be added under a `spotify` Cargo feature. Spotify's `/search` endpoint was not on the November 2024 deprecation list and reportedly remains available for new client-credentials apps; this is an empirical bet rather than a confirmed fact and is explicitly out of v1 scope.

**No anchor songs, no banger pacing, no AC2.** All three were considered and dropped during clarification. Each could return as a v2 feature without touching the algorithm core: anchors as a fixed-position constraint in `CostContext`, banger pacing as an additional per-position term, time targets as a CI assertion. The current architecture leaves room for these without rework.

**Implementation scoping.** Seven implementation phases, within the 8-phase limit for a single implementation plan.
