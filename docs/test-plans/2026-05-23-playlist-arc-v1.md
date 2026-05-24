# Human Test Plan — playlist-arc-v1

**Generated:** 2026-05-23
**Branch:** `playlist-arc-v1`
**Commit:** `5f93b60a2354537e215e59491b6d12d72da1d466`

Automated coverage validated: **37/37 criteria covered, 0 missing**. This plan covers the manual verification needed for items the automated suite cannot exercise (real binary smoke, miette panic rendering, network silence under offline conditions) plus end-to-end runs that combine multiple criteria.

## Prerequisites

- Rust toolchain installed (`cargo --version` works).
- Working directory: `/Users/y/Apps/music/order_playlist/`
- Branch checked out: `playlist-arc-v1` at HEAD `5f93b60`.
- `cargo build --release` completes without warnings.
- `cargo test` completes with all tests passing (no `--features live-network`).
- `cargo clippy --all-targets -- -D warnings` is clean.
- `cargo fmt --check` is clean.

## Phase 1: Smoke test with the bundled fixture

| Step | Action | Expected |
|------|--------|----------|
| 1.1 | `mkdir -p /tmp/order_playlist-smoke && cp tests/fixtures/small_party.csv /tmp/order_playlist-smoke/in.csv && cp tests/fixtures/small_party.cache.json /tmp/order_playlist-smoke/in.cache.json` | Files copied. |
| 1.2 | `./target/release/order_playlist --input /tmp/order_playlist-smoke/in.csv --output /tmp/order_playlist-smoke/out.csv --seed 42` | Process exits 0. Stdout shows: (a) "Energy arc" chart with `#` columns and `0.0`–`1.0` row labels, (b) "Summary" block with resolved/unresolved/seed/before/after cost/breakdown/remaining clashes. No "error:" line on stderr. |
| 1.3 | `head -1 /tmp/order_playlist-smoke/out.csv` | Equals exactly: `position,title,artist,tempo,key,mode,energy,danceability,valence,loudness,isrc` |
| 1.4 | `wc -l /tmp/order_playlist-smoke/out.csv` | 11 lines (1 header + 10 rows). |
| 1.5 | `awk -F',' 'NR>1 {print NF}' /tmp/order_playlist-smoke/out.csv | sort -u` | Single value `11`. |
| 1.6 | Re-run step 1.2 saving to a different output, then diff the two outputs. | No diff — byte-identical output (validates AC5.1 visually). |

## Phase 2: AC8.3 — bare-bones binary clearly errors

| Step | Action | Expected |
|------|--------|----------|
| 2.1 | `cargo build --no-default-features` | Build succeeds with no warnings/errors. |
| 2.2 | `./target/debug/order_playlist --input /tmp/x.csv --output /tmp/y.csv 2>&1; echo "exit=$?"` | Stderr contains `no resolver/feature source compiled in; build with --features musicbrainz,reccobeats`. Process exits non-zero. |
| 2.3 | `cargo build` (restore default features) | Default binary rebuilt; ready for further tests. |

## Phase 3: AC9.3 — miette panic rendering (manual only)

| Step | Action | Expected |
|------|--------|----------|
| 3.1 | In a scratch branch, insert `panic!("forced panic for AC9.3 demo")` near the top of `main.rs::main` and `cargo run --release`. | Stderr renders miette's fancy formatting: ANSI-colored error block, file:line context, `code` diagnostic identifier, and `help` text. NOT a plain `thread 'main' panicked at …` stack trace. |
| 3.2 | Revert the scratch change. | No diff to commit. |

## Phase 4: AC9.3 — diagnostic rendering on real errors

| Step | Action | Expected |
|------|--------|----------|
| 4.1 | `./target/release/order_playlist --input /nonexistent.csv --output /tmp/out.csv` | Stderr contains `error: input file not found: /nonexistent.csv`. Exit code 3 (run `echo $?`). |
| 4.2 | Create `/tmp/order_playlist-bad/bad_header.csv` containing `foo,bar\nx,y\n`, then `./target/release/order_playlist --input /tmp/order_playlist-bad/bad_header.csv --output /tmp/o.csv`. | Stderr error mentions missing column(s) `title` and/or `artist`. Exit code 3. |
| 4.3 | Create `/tmp/order_playlist-bad/header_only.csv` containing `title,artist\n` then run. | Exit code 3; error mentions "zero data rows". |
| 4.4 | Create `/tmp/order_playlist-bad/corrupt.cache.json` containing `{ not valid json`, point a run at it via `--cache /tmp/order_playlist-bad/corrupt.cache.json --input tests/fixtures/small_party.csv --output /tmp/o.csv`. | Exit code 4 (CacheError); stderr explains corrupt cache and suggests deletion. |
| 4.5 | Same as 4.4 but cache content `{"version": 99, "resolutions": [], "features": {}}`. | Exit code 4; stderr mentions version mismatch (found 99, expected 1). |

## End-to-End: Deterministic warm-cache run produces a sensible playlist

Purpose: validates AC1 + AC2 + AC3 + AC5 + AC6 + AC7 simultaneously on the real binary (not just the library entry point used in integration tests).

Steps:
1. From a clean temp dir, copy `tests/fixtures/small_party.csv` and `tests/fixtures/small_party.cache.json` next to each other as `in.csv` and `in.cache.json`.
2. Run: `./target/release/order_playlist --input in.csv --output out.csv --seed 42 --artist-window 4`.
3. Verify stdout contains:
   - The `Energy arc` header followed by ASCII bar chart with `#` cells.
   - A `Summary` block with `resolved: 10`, `unresolved: 0`, `seed: 42 (supplied)`, before/after totals, per-term breakdown for arc/camelot/tempo/energy/artist, and `artist clashes remaining: 0`.
4. Inspect `out.csv`: header exactly matches the AC1.1 string; 10 data rows; each row has 11 comma-separated fields; positions 1..10 in order; ISRC column non-empty.
5. Skim the `artist` column visually — no two adjacent (or within 4 positions of each other) rows share an artist.
6. Re-run the same command into `out2.csv`. `diff out.csv out2.csv` shows no output (AC5.1).
7. Re-run with `--seed 1` into `out_s1.csv` and `--seed 99999` into `out_s2.csv`. `diff` should show at least one pair of seed values producing different orderings (AC5.3 — may need to try multiple pairs if the cost landscape has a dominant minimum).
8. Run with a warm cache containing all 10 entries; capture network with `lsof -i` or `tcpdump` during the run to confirm zero outbound connections (AC7.1, manual confirmation).

## End-to-End: Partial unresolved with sidecar re-feed

Purpose: validates AC4 end-to-end including the re-feed pathway.

Steps:
1. Build a CSV `bad.csv` containing the 5 known-resolvable titles from `tests/fixtures/with_bad_rows.csv` plus 3 fabricated rows (e.g., `Not A Real Song,Not A Real Artist`). Use an empty cache.
2. Run: `./target/release/order_playlist --input bad.csv --output out.csv --seed 42`.
3. Expect exit 0 (partial success). `out.csv` contains the 5 resolved tracks. `unresolved.csv` (next to `out.csv`) has 4 lines: header `title,artist,reason` plus 3 rows.
4. Re-feed: `./target/release/order_playlist --input unresolved.csv --output out2.csv --seed 42 --cache /tmp/empty.cache.json`. Expect this run to also fail to resolve them and produce another unresolved.csv with the same 3 rows (proving AC4.5 round-trip works through the real binary).
5. Replace `bad.csv` with a CSV where the resolver knows zero entries (e.g., `unresolved.csv` from step 3 if you have no network). Run again with an empty cache and offline. Expect exit code 5 (NothingResolved).

## Human Verification Required

| Criterion | Why Manual | Steps |
|-----------|------------|-------|
| AC8.2 | Architecture-only; verified by grep gate at PR review. | Run `grep -rn 'crate::adapters\|use crate::adapters\|use order_playlist::adapters' src/algo/ src/domain/ src/cli/` — expect zero matches (a single doc comment in `src/domain/mod.rs` that *mentions* the forbidden import is fine). Confirm `Resolver` and `FeatureSource` traits in `src/adapters/mod.rs` are the only adapter surfaces touched by `run.rs`. |
| AC8.3 (runtime) | Bare-bones binary cannot construct `RunDeps`; no in-process test can exercise it. | Steps 2.1–2.3 above. |
| AC9.3 (runtime) | miette renders ANSI to process-global stderr; awkward to capture in-process. | Steps 3.1–3.2 above, supplemented by Steps 4.1–4.5 to confirm the diagnostic codes and help strings render on real error paths. |
| AC5.4 (log) | Args::resolve unit-tests the seed derivation; log emission verified by visual inspection. | Run `./target/release/order_playlist --input … --output … -v` without `--seed`; observe INFO log `no --seed supplied; derived from system time` with the derived seed. |
| AC9.2 (log) | Adapters emit info-level tracing on every network call; verified by visual log inspection on real network runs. | Run `./target/release/order_playlist --input … -vv` against a cold-cache input and observe INFO events `musicbrainz lookup` and `reccobeats batch` on stderr. |

## Traceability

| Acceptance Criterion | Automated Test | Manual Step |
|----------------------|----------------|-------------|
| AC1.1 | `csv_io::tests::write_output_happy_path`; `tests/end_to_end.rs::end_to_end_warm_cache_produces_output` | Phase 1 (1.3–1.5); E2E warm-cache step 4 |
| AC1.2 | `csv_io::tests::read_input_ac1_2_extra_columns_tolerated` | — |
| AC1.3 | `csv_io::tests::read_input_ac1_3_not_found` | Phase 4.1 |
| AC1.4 | `csv_io::tests::read_input_ac1_4_missing_artist` + `_missing_both` | Phase 4.2 |
| AC1.5 | `csv_io::tests::read_input_ac1_5_header_only` + `_empty_file` | Phase 4.3 |
| AC1.6 | `csv_io::tests::write_output_ac1_6_missing_parent_dir` | — |
| AC2.1 | `tests/end_to_end.rs::end_to_end_warm_cache_produces_output` | Phase 1.2 (chart appears on stdout) |
| AC2.2 | `chart::tests::renders_for_n_10_climbing_arc` (snapshot) | — |
| AC2.3 | `chart::tests::renders_for_n_1`, `n_2`, `n_3`, `empty_renders_without_panic` | — |
| AC3.1 | `anneal::tests::artist_spacing_respected_default_window`; `end_to_end::end_to_end_warm_cache_produces_output` | E2E warm-cache step 5 |
| AC3.2 | `args::tests::artist_window_zero_accepted`; `cost::tests::cost_weights_with_disabled_artist_window` | — |
| AC3.3 | `report::tests::count_artist_clashes_with_window_4_inclusive` + `_window_3_excludes_distance_4` | — |
| AC3.4 | `report::tests::count_artist_clashes_with_window_4_inclusive`; `anneal::tests::permutation_property`; report `summary_happy` snapshot | E2E warm-cache step 3 (summary shows clash count) |
| AC3.5 | `anneal::tests::artist_spacing_respected_default_window` (proptest) | — |
| AC4.1 | `tests/unresolved.rs::partial_unresolved_exits_zero_and_writes_sidecar`; `csv_io::tests::write_unresolved_happy_path` | E2E partial unresolved step 3 |
| AC4.2 | `tests/unresolved.rs::partial_unresolved_exits_zero_and_writes_sidecar` | E2E partial unresolved step 3 |
| AC4.3 | Structural (`tracing::warn!` events) — covered by `run.rs` branches exercised in unresolved test | — |
| AC4.4 | `tests/unresolved.rs::all_unresolved_exits_5` | E2E partial unresolved step 5 |
| AC4.5 | `csv_io::tests::write_unresolved_ac4_5_round_trip` | E2E partial unresolved step 4 |
| AC5.1 | `tests/determinism.rs::two_runs_same_seed_produce_byte_identical_output` | Phase 1.6; E2E warm-cache step 6 |
| AC5.2 | `tests/determinism.rs::two_runs_same_seed_produce_byte_identical_unresolved` | — |
| AC5.3 | `tests/determinism.rs::different_seeds_may_produce_different_orderings` | E2E warm-cache step 7 |
| AC5.4 | `args::tests::parses_required_args` + `system_time_seed_derivation_when_not_supplied` | AC5.4 log verification in Human Verification Required |
| AC6.1 | `report::tests::happy_path_summary_snapshot` | E2E warm-cache step 3 |
| AC6.2 | `report::tests::happy_path_summary_snapshot` (insta snapshot `summary_happy`) | — |
| AC6.3 | `report::tests::no_unresolved_omits_sidecar_pointer`; `happy_path_summary_snapshot` | E2E warm-cache step 3; E2E partial unresolved step 3 |
| AC7.1 | `tests/zero_network.rs::warm_cache_does_not_invoke_adapters` | E2E warm-cache step 8 (manual offline run with lsof/tcpdump) |
| AC7.2 | `cache::tests::cache_save_atomic_preserves_existing_on_tmp_failure` + `_determinism` | — |
| AC7.3 | `cache::tests::cache_load_version_mismatch`; `tests/exit_codes.rs::version_mismatch_cache_load_or_exit_returns_cache_error` | Phase 4.5 |
| AC7.4 | `cache::tests::cache_load_empty_json_corrupts` | Phase 4.4 |
| AC8.1 | `.github/workflows/ci.yml` matrix (`--no-default-features --features musicbrainz,reccobeats`) | `cargo build --no-default-features --features musicbrainz,reccobeats && cargo test --no-default-features --features musicbrainz,reccobeats` |
| AC8.2 | Architecture-only (no test) | Human Verification Required |
| AC8.3 | CI build matrix (`--no-default-features`) + CI smoke step | Phase 2.1–2.3 |
| AC9.1 | Compile-time gate `#![cfg(all(feature="…", feature="live-network"))]` on `tests/musicbrainz_live.rs` + `tests/reccobeats_live.rs`; CI omits `live-network` | — |
| AC9.2 | Structural (`tracing::info!` at search/lookup/batch) — paths exercised by integration tests | AC9.2 log verification in Human Verification Required |
| AC9.3 (compile) | `main.rs` calls `miette::set_panic_hook()`; `errors.rs` derives `miette::Diagnostic`; `errors::tests::*` construction tests | Phase 3.1–3.2; Phase 4.1–4.5 |
