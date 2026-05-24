# playlistize

Last verified: 2026-05-23

Rust binary crate that reorders a CSV playlist along a target energy arc.
This file captures the cross-cutting architectural rules that aren't
obvious from reading any single module. For verification/review rules and
the "what blocks a merge" list, see `implementation-plan-guidance.md`.
For domain terminology and v1 scope, see `design-plan-guidance.md`. Do
not duplicate either file's content here.

## Tech Stack

- Rust 2021, async via `tokio` (full).
- HTTP: `reqwest` (rustls).
- Serialization: `serde` + `serde_json` + `csv`.
- RNG: `rand` + `rand_chacha::ChaCha20Rng` (seedable for determinism).
- Errors: `thiserror` for typed errors, `miette` for diagnostics (with
  source spans on `InputError::MissingColumn`).
- CLI: `clap` derive. Logging: `tracing` + `tracing-subscriber`.
- Tests: `proptest`, `insta`, `wiremock`, `test-case`, `tempfile`.

All dependency versions are pinned with `=x.y.z` in `Cargo.toml`. Do not
relax pins without an explicit reason.

## Commands

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test` (defaults: `musicbrainz,reccobeats` features)
- `cargo test --no-default-features` (proves the algo/domain core builds
  with zero adapters compiled in)
- `cargo test --features live-network -- --ignored`-equivalent: live
  network tests are gated behind `#![cfg(feature = "live-network")]`,
  not `#[ignore]`. Run them explicitly with
  `cargo test --features live-network,musicbrainz,reccobeats`.
- `cargo build --release`

## Module Layout (FCIS)

```
src/
├── lib.rs              # Re-exports run(), ExitCode, RunDeps, RunReport
├── main.rs             # Binary entry: clap + tracing init + std::process::exit
├── run.rs              # Orchestration: shell that wires deps to the pure core
├── domain/             # Pure: types, newtypes, Camelot codes
├── algo/               # Pure: cost, arc, anneal, camelot distance table
├── adapters/           # Shell: csv_io, cache, musicbrainz, reccobeats
├── cli/                # Presentation: args, chart, summary report
└── errors.rs           # InputError, CacheError, AdapterError, MusicBrainzError,
                        # ReccoBeatsError (all thiserror + miette::Diagnostic)
```

Each file starts with a `// pattern: Functional Core` or
`// pattern: Imperative Shell` comment plus a one-line reason. Honor it
when adding files.

### FCIS invariants (block on these)

- `src/domain/` MUST NOT import `std::fs`, `std::io`, `tokio`, `reqwest`,
  or anything from `crate::adapters` / `crate::cli`. Stated at top of
  `src/domain/mod.rs`.
- `src/algo/` MUST be zero IO, zero async, zero adapter imports. Also
  bans `anyhow::Result` — the algorithm is pure-by-construction and has
  no fallible surface. Stated at top of `src/algo/mod.rs`.
- `src/adapters/` is the only place HTTP, filesystem, and JSON IO happen.
- `eprintln!` / `println!` are allowed ONLY in `src/main.rs` and
  `src/cli/`. `run()` is permitted to write the chart + summary to
  stdout (those are deliverables) but never writes errors to stderr —
  it returns a `RunReport { message }` for `main.rs` to print.

## Feature Flags

| Flag            | Default | Effect                                           |
|-----------------|---------|--------------------------------------------------|
| `musicbrainz`   | yes     | Compiles `adapters::musicbrainz` + ISRC resolver |
| `reccobeats`    | yes     | Compiles `adapters::reccobeats` + features fetch |
| `live-network`  | no      | Enables `tests/musicbrainz_live.rs` and `tests/reccobeats_live.rs` (real HTTP) |

Pattern for adding a new provider: add a feature flag, add a
`#[cfg(feature = "...")] pub mod ...;` and matching `pub use` in
`src/adapters/mod.rs`, and implement the `Resolver` or `FeatureSource`
trait. Nothing in `algo/` or `domain/` should change.

When `--no-default-features` is built, `main.rs`'s `build_deps()` returns
an error (`AC8.3`) — the binary still compiles and runs, it just refuses
to start without at least one provider compiled in.

## Determinism Anchors

- `Cache` uses `BTreeMap<TrackQuery, TrackId>` and
  `BTreeMap<TrackId, TrackFeatures>` so JSON serialization is byte-stable
  across runs. `CACHE_VERSION` (currently `1`) gates schema-breaking
  changes; bumping it is a deliberate decision.
- `Cache::save_atomic` writes to `path.json.tmp` then `std::fs::rename`s
  into place. A mid-run SIGKILL never produces a partial cache file.
- Resolver cache semantics: cache BOTH successes and explicit failures
  (empty `TrackId::empty()` sentinel) so unresolvable queries don't
  re-hit the network. Feature source cache semantics: cache successes
  only — transient `None` results re-try on next run.
- Annealing uses `ChaCha20Rng::seed_from_u64(args.seed)`; `optimize()` is
  generic over `R: rand::Rng` so tests can inject deterministic RNGs.
- `run()` partitions queries against the cache BEFORE calling the
  resolver (and IDs against the feature cache before calling the feature
  source). This is the AC7.1 invariant: a fully warm cache must produce
  zero network calls. The integration test in `tests/zero_network.rs`
  enforces this with `PanicOnCallResolver` / `PanicOnCallFeatureSource`
  doubles in `tests/support/in_memory.rs`. Do not collapse the partition
  pass into the resolver — the panic-on-call doubles exist precisely to
  catch that regression.

## Exit Codes

Defined as `pub enum ExitCode` in `src/run.rs`. Semantic; tests in
`tests/exit_codes.rs` pin the integer values.

| Code | Variant            | Meaning                                       |
|------|--------------------|-----------------------------------------------|
| 0    | `Success`          | All tracks resolved, annealed, written        |
| 2    | `BadArgs`          | Clap rejected the CLI (clap exits directly)   |
| 3    | `InputError`       | CSV missing / malformed / zero rows           |
| 4    | `CacheError`       | Cache corrupt or schema-version mismatch      |
| 5    | `NothingResolved`  | Input loaded but no track resolved (AC4.4)    |
| 6    | `NetworkExhausted` | Adapter exhausted retries (rate limit, etc.)  |

Map new failure classes to a new variant rather than collapsing them to
`1`. The P7 review specifically flagged "domain errors collapse to exit
1, breaking AC1.3–1.6" as a Critical regression — do not reintroduce it.

## `run()` vs `main.rs`

`pub fn run(args: ResolvedArgs, deps: RunDeps) -> Result<(ExitCode, RunReport), miette::Report>`
is the library-visible orchestration entry point. Integration tests call
it directly with `InMemoryResolver` / `InMemoryFeatureSource` (or the
panic-on-call doubles).

`main.rs` is the only production code that:
- installs the `miette::set_panic_hook()`,
- initializes the `tracing_subscriber` (so integration tests don't
  collide with a global subscriber),
- loads the cache (single instance — `RunDeps::cache` is an
  `Arc<Mutex<Cache>>` shared with `run()` so we don't double-load and
  race on writes),
- calls `std::process::exit(code as i32)`.

When adding orchestration logic, put it in `run.rs`. `main.rs` stays
small — it's effectively untestable by design.

## Testing Patterns Used Here

- `proptest` for delta-cost-vs-full-cost equivalence and artist-spacing
  reduction properties. Regressions are committed under
  `proptest-regressions/algo/`.
- `insta` snapshots for ASCII chart and summary report formatting
  (`src/cli/snapshots/`).
- `wiremock` for MusicBrainz / ReccoBeats adapter contract tests
  (in-file under `#[cfg(test)] mod tests`, NOT in `tests/`).
- Perf-sensitive tests use `#[cfg(not(debug_assertions))]` instead of
  `#[ignore]` so they run automatically under `cargo test --release` and
  never silently skip in CI. Example: `algo::anneal` SA throughput
  assertion.
- `tests/fixtures/build_small_party_cache.rs` is a separate binary
  (`required-features = []`) that rebuilds `small_party.cache.json`
  against the live MusicBrainz / ReccoBeats APIs. The fixture itself is
  committed so default `cargo test` is offline.
- Live-network tests are gated via `#![cfg(all(feature = "<adapter>", feature = "live-network"))]`
  at the file level (not `#[ignore]`); when the features are not enabled
  the file is structurally absent from `cargo test`'s output. This satisfies
  AC9.1's no-silent-skip intent: a cfg-excluded file produces no test
  artifacts, which is structurally honest (contrast: `#[ignore]` hides the
  test from the count and produces a mysterious "skipped" line).

## Boundaries

- Safe to edit: everything under `src/`, `tests/`, `docs/`.
- Do not edit: `implementation-plan-guidance.md`, `design-plan-guidance.md`,
  `SEED.md`, `proptest-regressions/` (regenerate, don't hand-edit),
  `Cargo.lock` (let cargo manage it).
- `tests/fixtures/small_party.cache.json` is regenerated by the
  `build_small_party_cache` binary; don't hand-edit values.
