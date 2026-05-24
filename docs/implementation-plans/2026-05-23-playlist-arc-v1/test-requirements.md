# Test Requirements — playlist-arc-v1

Generated 2026-05-23 from the design plan and implementation phases.

## Coverage summary

- Total ACs: 38
- Automated: 37 (unit: 17, integration: 16, property: 3, snapshot: 4 — some tests counted under more than one type)
- Human verification: 1 (architecture-only — AC8.2)
- Partial human verification: 2 (AC8.3 startup-error behavior, AC9.3 panic-hook rendering)

## Acceptance criteria mapping

### playlist-arc-v1.AC1: Input CSV is read, reordered, augmented with features, and written

#### playlist-arc-v1.AC1.1 Success
- **Type:** unit (snapshot) + integration
- **Test:** `src/adapters/csv_io.rs::tests::write_output` header snapshot via `insta` (Phase 4 Task 3) — header equals `position,title,artist,tempo,key,mode,energy,danceability,valence,loudness,isrc` and rows have 11 fields.
- **Test:** `tests/end_to_end.rs::end_to_end_warm_cache_produces_output` (Phase 7 Task 7) — header equals the AC1.1 string and total rows == `header + 10`.

#### playlist-arc-v1.AC1.2 Success
- **Type:** unit
- **Test:** `src/adapters/csv_io.rs::tests::read_input` case "AC1.2 extra columns" (Phase 4 Task 2)
- **Asserts:** Input with an extra column parses to one `TrackQuery`; extras are dropped.

#### playlist-arc-v1.AC1.3 Failure
- **Type:** unit
- **Test:** `src/adapters/csv_io.rs::tests::read_input` case "AC1.3 not found" (Phase 4 Task 2)
- **Asserts:** Non-existent path returns `Err(InputError::NotFound { path })`.

#### playlist-arc-v1.AC1.4 Failure
- **Type:** unit
- **Test:** `src/adapters/csv_io.rs::tests::read_input` cases "AC1.4 missing artist" and "AC1.4 missing both" (Phase 4 Task 2)
- **Asserts:** Missing required headers return `InputError::MissingColumn` with the missing names and a miette source-span over the header.

#### playlist-arc-v1.AC1.5 Failure
- **Type:** unit
- **Test:** `src/adapters/csv_io.rs::tests::read_input` cases "AC1.5 header-only" and "AC1.5 empty file" (Phase 4 Task 2)
- **Asserts:** Header-only returns `InputError::NoRows`; empty file returns `InputError::Csv`.

#### playlist-arc-v1.AC1.6 Edge
- **Type:** unit
- **Test:** `src/adapters/csv_io.rs::tests::write_output` "missing parent dir" case (Phase 4 Task 3)
- **Asserts:** Writing to a path with a non-existent parent returns `InputError::MissingParentDir`; no directory is created.

### playlist-arc-v1.AC2: ASCII energy-arc chart is printed to stdout

#### playlist-arc-v1.AC2.1 Success
- **Type:** integration
- **Test:** `tests/end_to_end.rs::end_to_end_warm_cache_produces_output` (Phase 7 Task 7) — `src/run.rs::run` (Phase 7 Task 5) calls `render_arc` and prints to stdout; success exit proves the path executed.

#### playlist-arc-v1.AC2.2 Success
- **Type:** snapshot (unit)
- **Test:** `src/cli/chart.rs::tests::renders_for_n_10_climbing_arc` via `insta::assert_snapshot!("arc_n10_climbing", ...)` (Phase 7 Task 3)
- **Asserts:** Chart bytes are byte-stable across runs.

#### playlist-arc-v1.AC2.3 Edge
- **Type:** snapshot (unit)
- **Tests:** `src/cli/chart.rs::tests::renders_for_n_1`, `renders_for_n_2`, `renders_for_n_3`, `empty_renders_without_panic` (Phase 7 Task 3)
- **Asserts:** Chart renders without panicking for `n ∈ {0, 1, 2, 3}`; snapshots stable.

### playlist-arc-v1.AC3: No two songs share an artist within `--artist-window` positions

#### playlist-arc-v1.AC3.1 Success
- **Type:** property + integration
- **Test:** `src/algo/anneal.rs::tests::artist_spacing_respected_default_window` (Phase 3 Task 9) — algorithm enforces the constraint with default window 4.
- **Test:** `tests/end_to_end.rs::end_to_end_warm_cache_produces_output` (Phase 7 Task 7) — full pipeline runs with default window.

#### playlist-arc-v1.AC3.2 Success
- **Type:** unit
- **Test:** `src/cli/args.rs::tests::artist_window_zero_accepted` (Phase 7 Task 1) — clap accepts `--artist-window=0`.
- **Test:** `src/algo/cost.rs::tests` `CostWeights { artist_window: 0, .. }` case (Phase 3 Task 5) — cost function applies no clash penalty when window is 0.

#### playlist-arc-v1.AC3.3 Success
- **Type:** unit
- **Test:** `src/cli/report.rs::tests::count_artist_clashes_window_3_excludes_distance_4` and `count_artist_clashes_with_window_4_inclusive` (Phase 7 Task 4)
- **Asserts:** Arbitrary positive window honored; boundary inclusive at distance == window.

#### playlist-arc-v1.AC3.4 Edge
- **Type:** unit
- **Test:** `src/cli/report.rs::tests::count_artist_clashes_with_window_4_inclusive` (Phase 7 Task 4) + the `remaining_clashes` field rendered by `summary_happy` snapshot (Phase 7 Task 4). `src/algo/anneal.rs::tests` permutation property (Phase 3 Task 8) guarantees `optimize` returns a valid permutation for any input — annealer never crashes on infeasible inputs.

#### playlist-arc-v1.AC3.5 Verification
- **Type:** property
- **Test:** `src/algo/anneal.rs::tests::artist_spacing_respected_default_window` (Phase 3 Task 9) — proptest 8 cases over random seeds; for default `CostWeights`, no two tracks within 4 positions share an artist after `optimize`.

### playlist-arc-v1.AC4: Unresolved tracks are logged and written to a sidecar CSV

#### playlist-arc-v1.AC4.1 Success
- **Type:** integration + unit
- **Test:** `tests/unresolved.rs::partial_unresolved_exits_zero_and_writes_sidecar` (Phase 7 Task 9) — sidecar exists with `title,artist,reason` header and one row per unresolved.
- **Test:** `src/adapters/csv_io.rs::tests::write_unresolved` (Phase 4 Task 4) — `write_unresolved` produces a valid CSV with three columns.

#### playlist-arc-v1.AC4.2 Success
- **Type:** integration
- **Test:** `tests/unresolved.rs::partial_unresolved_exits_zero_and_writes_sidecar` (Phase 7 Task 9)
- **Asserts:** Exit `ExitCode::Success` even with 3 of 8 unresolved.

#### playlist-arc-v1.AC4.3 Success
- **Type:** structural (source-level) + integration
- **Test:** `src/run.rs::run` (Phase 7 Task 5) emits `tracing::warn!` on all three unresolved branches (cached prior failure, resolver-reported, feature `None`); `src/adapters/musicbrainz.rs` `resolve_many` (Phase 5 Task 5) emits `tracing::warn!` per unresolved. Branches covered by `tests/unresolved.rs::partial_unresolved_exits_zero_and_writes_sidecar` (Phase 7 Task 9). Event content not asserted — per project convention tracing events do not require coverage.

#### playlist-arc-v1.AC4.4 Failure
- **Type:** integration
- **Test:** `tests/unresolved.rs::all_unresolved_exits_5` (Phase 7 Task 9)
- **Asserts:** Resolver knowing zero queries returns `ExitCode::NothingResolved` (5).

#### playlist-arc-v1.AC4.5 Edge
- **Type:** unit
- **Test:** `src/adapters/csv_io.rs::tests::write_unresolved` AC4.5 round-trip case (Phase 4 Task 4)
- **Asserts:** `write_unresolved` → `read_input` round-trip returns original `Vec<TrackQuery>`; `reason` is ignored on re-feed.

### playlist-arc-v1.AC5: Deterministic runs with `--seed`

#### playlist-arc-v1.AC5.1 Success
- **Type:** integration
- **Test:** `tests/determinism.rs::two_runs_same_seed_produce_byte_identical_output` (Phase 7 Task 8)
- **Asserts:** Two runs with seed 42, identical input and cache, produce byte-identical `output.csv`.

#### playlist-arc-v1.AC5.2 Success
- **Type:** integration
- **Test:** `tests/determinism.rs::two_runs_same_seed_produce_byte_identical_unresolved` (Phase 7 Task 8)
- **Asserts:** Two runs with skipped queries produce byte-identical `unresolved.csv`.

#### playlist-arc-v1.AC5.3 Success
- **Type:** integration
- **Test:** `tests/determinism.rs::different_seeds_produce_different_orderings` (Phase 7 Task 8)
- **Asserts:** Seeds 1 vs 2 produce different `output.csv` bytes. Backed by algorithm-level determinism + improvement tests in `src/algo/anneal.rs::tests` (Phase 3 Task 8).

#### playlist-arc-v1.AC5.4 Edge
- **Type:** unit
- **Test:** `src/cli/args.rs::tests::parses_required_args` (Phase 7 Task 1)
- **Asserts:** Omitting `--seed` sets `seed_was_supplied = false` and derives a value from `SystemTime`. `src/main.rs::main` (Phase 7 Task 6) logs the chosen seed at INFO when `!seed_was_supplied` — emission path covered by the same unit test.

### playlist-arc-v1.AC6: Summary report is emitted to stdout

#### playlist-arc-v1.AC6.1 Success
- **Type:** snapshot (unit)
- **Test:** `src/cli/report.rs::tests::happy_path_summary_snapshot` (Phase 7 Task 4)
- **Asserts:** Report includes resolved/unresolved counts, seed, before/after total cost, before/after arc deviation, per-term breakdown (arc, camelot, tempo, energy, artist), remaining clashes — fixed by `summary_happy` snapshot.

#### playlist-arc-v1.AC6.2 Success
- **Type:** snapshot
- **Test:** `src/cli/report.rs::tests::happy_path_summary_snapshot` via `insta::assert_snapshot!("summary_happy", ...)` (Phase 7 Task 4).

#### playlist-arc-v1.AC6.3 Edge
- **Type:** unit
- **Test:** `src/cli/report.rs::tests::no_unresolved_omits_sidecar_pointer` (Phase 7 Task 4)
- **Asserts:** With `unresolved == 0` the report omits `see unresolved.csv`; `happy_path_summary_snapshot` (with `unresolved == 2`) includes the sidecar pointer.

### playlist-arc-v1.AC7: Cache hits zero network on warm runs

#### playlist-arc-v1.AC7.1 Success
- **Type:** integration
- **Test:** `tests/zero_network.rs::warm_cache_does_not_invoke_adapters` (Phase 7 Task 10)
- **Asserts:** Running with `PanicOnCallResolver` + `PanicOnCallFeatureSource` against the warm `small_party.cache.json` returns `ExitCode::Success` without panicking — proves `src/run.rs::run` cache-partition (Phase 7 Task 5) bypasses adapter calls.

#### playlist-arc-v1.AC7.2 Success
- **Type:** unit
- **Test:** `src/adapters/cache.rs::tests::save_atomic_preserves_existing_on_temp_failure` + the "save twice, bytes equal" assertion in the same module (Phase 4 Task 7)
- **Asserts:** Failed temp write leaves existing file unchanged; `BTreeMap` ordering produces byte-identical serialization across runs.

#### playlist-arc-v1.AC7.3 Failure
- **Type:** unit
- **Test:** `src/adapters/cache.rs::tests::load` "version 99" case (Phase 4 Task 5)
- **Asserts:** `Cache::load` on `"version": 99` returns `CacheError::VersionMismatch { found: 99, expected: 1 }`; `main.rs` propagates as exit 4.

#### playlist-arc-v1.AC7.4 Edge
- **Type:** unit
- **Test:** `src/adapters/cache.rs::tests::load` empty-file case (Phase 4 Task 5)
- **Asserts:** Corrupt JSON returns `CacheError::Corrupt` with underlying `serde_json::Error` attached via `#[source]`.

### playlist-arc-v1.AC8: Adapters are swappable via Cargo features

#### playlist-arc-v1.AC8.1 Success
- **Type:** integration (build matrix)
- **Test:** `.github/workflows/ci.yml` matrix entry `--no-default-features --features musicbrainz,reccobeats` (Phase 7 Task 11). Locally enforced by Phase 1 Task 5 and Phase 6 Task 8 verification.
- **Asserts:** Build succeeds and is behaviorally identical to default (default features == `["musicbrainz", "reccobeats"]`).

#### playlist-arc-v1.AC8.2 Success
- **Type:** architecture-only
- **Test:** No automated test exists — and this is correct, because there is nothing yet to test. Verification is structural: `algo/` + `domain/` import nothing from `adapters/` (grep gate in Phase 3 Task 10); `Resolver` + `FeatureSource` are the only trait surfaces the orchestration touches; `#[cfg(feature = "<provider>")]` gates confirm new providers can be added without touching `algo`/`domain`/`cli`. Reviewed at PR time; a future `spotify` adapter would prove it by example.

#### playlist-arc-v1.AC8.3 Edge
- **Type:** integration (build matrix) + human verification
- **Test (a — build succeeds):** `.github/workflows/ci.yml` matrix entry `--no-default-features` (Phase 7 Task 11) + Phase 1 Task 5 verification — `cargo build --no-default-features` succeeds.
- **Test (b — binary errors clearly):** Manual smoke step in Phase 7 Task 6 — running the bare-bones binary prints `no resolver/feature source compiled in; build with --features musicbrainz,reccobeats` to stderr. Error originates in `src/main.rs::build_deps` `#[cfg(not(all(...)))]` branch. Human-verified because no in-process test can exercise the bare-bones binary (it cannot construct `RunDeps` without adapter impls; subprocess testing is out of scope for v1).

### playlist-arc-v1.AC9: Cross-cutting

#### playlist-arc-v1.AC9.1
- **Type:** integration (negative)
- **Test:** Live-network tests are excluded via `#![cfg(all(feature = "<provider>", feature = "live-network"))]` on `tests/musicbrainz_live.rs` (Phase 5 Task 7) and `tests/reccobeats_live.rs` (Phase 6 Task 7). `.github/workflows/ci.yml` runs `cargo test` without `live-network` (Phase 7 Task 11).
- **Asserts:** No silent skips — files are excluded at compile time when feature off, so no `#[ignore]` is needed. Wiremock tests are localhost-only.

#### playlist-arc-v1.AC9.2
- **Type:** structural (source-level)
- **Test:** `src/adapters/musicbrainz.rs::resolve_one` emits `tracing::info!` at search + lookup (Phase 5 Task 5); `src/adapters/reccobeats.rs::features_for` emits `tracing::info!` per batch (Phase 6 Task 5). Paths exercised by `src/adapters/musicbrainz.rs::integration_tests::happy_path_search_then_lookup_returns_isrc` (Phase 5 Task 6) and `src/adapters/reccobeats.rs::integration_tests::happy_path_batch_of_two` (Phase 6 Task 6). Event content reviewed at PR time, not snapshotted.

#### playlist-arc-v1.AC9.3
- **Type:** unit (compile-time wiring) + human verification
- **Test:** `src/main.rs::main` calls `miette::set_panic_hook()` first (Phase 7 Task 6); `src/errors.rs` types derive `miette::Diagnostic` with `#[diagnostic(code, help)]` (Phase 4 Task 1, Phase 5 Task 2, Phase 6 Task 2). Construction tests in `src/errors.rs::tests::*` confirm diagnostics compile.
- **Asserts (runtime, human-verified):** A `panic!` produces miette's fancy stderr output. No automated test — miette renders ANSI to process-global stderr that's awkward to capture in-process; acceptable for v1.
