# Playlist Arc v1 — Phase 7: CLI Integration, Presentation, End-to-End Tests

**Goal:** Wire `main.rs` to orchestrate the pipeline, parse args via clap, render outputs (CSV + ASCII chart + summary report), set semantic exit codes, and verify every DoD criterion with integration tests.

**Architecture:** `main.rs` is the only file allowed to use `anyhow::Result` and `miette::Report`. The orchestration is async at the adapter boundary, then synchronous through the algorithm. Tests in `tests/` exercise the full pipeline with in-memory test doubles (no network) and a couple of `live-network`-gated runs against real APIs.

**Tech Stack:** Rust, `clap` (derive), `tracing-subscriber` (env-filter + fmt), `miette` (panic hook + fancy renderer), `insta` (snapshot tests), `tokio` (async runtime).

**Scope:** Phase 7 of 7 from `/Users/y/Apps/music/order_playlist/docs/design-plans/2026-05-23-playlist-arc-v1.md`.

**Codebase verified:** 2026-05-23 — Phases 1–6 will be in place: domain types, pure algo, IO adapters, both resolver/feature-source implementations, in-memory test doubles. `src/cli/mod.rs` exists as a stub; `args.rs`, `chart.rs`, `report.rs` do not. `src/main.rs` is a `fn main() {}` stub from Phase 1.

**Project guidance:** `/Users/y/Apps/music/order_playlist/implementation-plan-guidance.md`. Key rules for this phase: integration test on a 10-song CSV running end-to-end through cached path; AC9.3 miette panic hook in `main.rs`; AC9.1 hermetic default `cargo test`; AC9.2 `tracing::info!` on network calls (verified by adapter phases, re-checked here).

---

## Acceptance Criteria Coverage

This phase implements and tests:

### playlist-arc-v1.AC1: Input CSV is read, reordered, augmented with features, and written
- **playlist-arc-v1.AC1.1 Success:** end-to-end output CSV with reordered N tracks and feature columns appended.
- **playlist-arc-v1.AC1.3 Failure:** input not found → exit 3.
- **playlist-arc-v1.AC1.4 Failure:** missing header → exit 3.
- **playlist-arc-v1.AC1.5 Failure:** zero-row input → exit 3.
- **playlist-arc-v1.AC1.6 Edge:** missing output parent dir → exit 3.

### playlist-arc-v1.AC2: ASCII energy-arc chart is printed to stdout
- **playlist-arc-v1.AC2.1 Success:** chart printed.
- **playlist-arc-v1.AC2.2 Success:** chart rendering is deterministic — snapshot via `insta`.
- **playlist-arc-v1.AC2.3 Edge:** renders for n ∈ {1, 2, 3}.

### playlist-arc-v1.AC3: No two songs share an artist within `--artist-window` positions
- **playlist-arc-v1.AC3.1 Success:** default window=4.
- **playlist-arc-v1.AC3.2 Success:** `--artist-window=0` disables.
- **playlist-arc-v1.AC3.3 Success:** arbitrary N honored.
- **playlist-arc-v1.AC3.4 Edge:** infeasible input completes; report surfaces remaining clashes.

### playlist-arc-v1.AC4: Unresolved tracks are logged and written to a sidecar CSV
- **playlist-arc-v1.AC4.1 Success:** sidecar with `title,artist,reason`.
- **playlist-arc-v1.AC4.2 Success:** unresolved doesn't cause non-zero exit if ≥ 1 resolved.
- **playlist-arc-v1.AC4.4 Failure:** all unresolved → exit 5.

### playlist-arc-v1.AC5: Deterministic runs with `--seed`
- **playlist-arc-v1.AC5.1 Success:** two consecutive runs produce byte-identical `output.csv`.
- **playlist-arc-v1.AC5.2 Success:** same for `unresolved.csv`.
- **playlist-arc-v1.AC5.4 Edge:** omitting `--seed` derives from system time and logs the chosen seed.

### playlist-arc-v1.AC6: Summary report is emitted to stdout
- **playlist-arc-v1.AC6.1 Success:** resolved/unresolved counts, before/after cost, per-term breakdown.
- **playlist-arc-v1.AC6.2 Success:** snapshot via `insta`.
- **playlist-arc-v1.AC6.3 Edge:** when unresolved > 0, report mentions sidecar.

### playlist-arc-v1.AC7: Cache hits zero network on warm runs
- **playlist-arc-v1.AC7.1 Success:** `tests/zero_network.rs` with `PanicOnCall*` doubles completes without panicking.

### playlist-arc-v1.AC8: Adapters are swappable via Cargo features
- **playlist-arc-v1.AC8.1 Success:** `cargo build --no-default-features --features musicbrainz,reccobeats` produces a working binary.
- **playlist-arc-v1.AC8.3 Edge:** `cargo build --no-default-features` produces a binary that errors clearly at startup.

### playlist-arc-v1.AC9: Cross-cutting
- **playlist-arc-v1.AC9.1:** zero network in default test runs.
- **playlist-arc-v1.AC9.3:** `main.rs` installs miette panic hook with source spans + help text.

---

## Task Overview

```
SUBCOMPONENT_A: CLI args + cli/mod.rs wiring (tasks 1-2)
SUBCOMPONENT_B: ASCII chart + summary report (tasks 3-4)
SUBCOMPONENT_C: Cross-phase additive changes (tasks 4a-4c) + run() orchestration + main.rs (tasks 5-6)
SUBCOMPONENT_D: Integration tests + fixtures (tasks 7-10)
SUBCOMPONENT_E: CI matrix + final verification (task 11)
```

**About Tasks 4a-4c.** Phase 7 needs three small additions to earlier phases — `CostContext::cost_breakdown` (goes in Phase 3's `algo/cost.rs`), `Cache::all_resolutions`/`all_features` (Phase 4's `adapters/cache.rs`), and a `CostBreakdown` re-export tweak (Phase 7's `cli/report.rs`). These are tracked as first-class tasks so each gets its own commit + unit test + re-verification of the phase whose code it touches — rather than being silent in-line edits.

---

<!-- START_SUBCOMPONENT_A (tasks 1-2) -->

<!-- START_TASK_1 -->
### Task 1: Implement clap Args struct in src/cli/args.rs

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/src/cli/args.rs`

**Implementation:**

```rust
//! CLI argument parser using `clap` derive.
//!
//! Defaults are filled in by `Args::resolve()` after parsing — clap
//! can't compute defaults that depend on other args (e.g.,
//! `unresolved` defaults to `output` with a different extension).

use std::path::PathBuf;

#[derive(clap::Parser, Debug)]
#[command(
    name = "order_playlist",
    version,
    about = "Reorder a CSV playlist to follow a target energy arc."
)]
pub struct Args {
    /// Input CSV file with `title,artist` columns.
    #[arg(long)]
    pub input: PathBuf,

    /// Output CSV file with reordered tracks + feature columns.
    #[arg(long)]
    pub output: PathBuf,

    /// Sidecar CSV for tracks that couldn't be resolved.
    /// Defaults to `unresolved.csv` next to `--output`.
    #[arg(long)]
    pub unresolved: Option<PathBuf>,

    /// Feature cache JSON. Defaults to `<input>.cache.json`.
    #[arg(long)]
    pub cache: Option<PathBuf>,

    /// Seed for the deterministic RNG. If absent, derived from system
    /// time and logged at INFO.
    #[arg(long)]
    pub seed: Option<u64>,

    /// Window for the artist-spacing constraint. `0` disables.
    #[arg(long, default_value_t = 4)]
    pub artist_window: u8,

    /// Increase log verbosity (-v=DEBUG, -vv=TRACE). Default is INFO.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// User-agent contact string for MusicBrainz (e.g., email or URL).
    /// MusicBrainz throttles aggressively without a real contact.
    #[arg(long, default_value = "anonymous@example.com")]
    pub musicbrainz_contact: String,
}

/// `Args` with all defaults resolved (no more `Option`s on
/// path/seed fields). Produced by `Args::resolve`.
pub struct ResolvedArgs {
    pub input: PathBuf,
    pub output: PathBuf,
    pub unresolved: PathBuf,
    pub cache: PathBuf,
    pub seed: u64,
    pub seed_was_supplied: bool,
    pub artist_window: u8,
    pub verbose: u8,
    pub musicbrainz_contact: String,
}

impl Args {
    pub fn resolve(self) -> ResolvedArgs {
        let unresolved = self.unresolved.unwrap_or_else(|| {
            self.output.parent()
                .map(|p| p.join("unresolved.csv"))
                .unwrap_or_else(|| PathBuf::from("unresolved.csv"))
        });
        let cache = self.cache.unwrap_or_else(|| {
            let mut p = self.input.clone();
            p.set_extension("cache.json");
            p
        });
        let (seed, seed_was_supplied) = match self.seed {
            Some(s) => (s, true),
            None => {
                let s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0);
                (s, false)
            }
        };
        ResolvedArgs {
            input: self.input,
            output: self.output,
            unresolved,
            cache,
            seed,
            seed_was_supplied,
            artist_window: self.artist_window,
            verbose: self.verbose,
            musicbrainz_contact: self.musicbrainz_contact,
        }
    }
}
```

**Testing:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn parses_required_args() {
        let args = Args::try_parse_from(["order_playlist", "--input", "in.csv", "--output", "out.csv"]).unwrap();
        let r = args.resolve();
        assert_eq!(r.input, PathBuf::from("in.csv"));
        assert_eq!(r.output, PathBuf::from("out.csv"));
        assert_eq!(r.artist_window, 4);
        assert!(!r.seed_was_supplied);
    }

    #[test]
    fn defaults_unresolved_to_output_dir() {
        let args = Args::try_parse_from(["order_playlist", "--input", "in.csv", "--output", "/tmp/out.csv"]).unwrap();
        let r = args.resolve();
        assert_eq!(r.unresolved, PathBuf::from("/tmp/unresolved.csv"));
    }

    #[test]
    fn defaults_cache_to_input_extension() {
        let args = Args::try_parse_from(["order_playlist", "--input", "/data/songs.csv", "--output", "out.csv"]).unwrap();
        let r = args.resolve();
        assert_eq!(r.cache, PathBuf::from("/data/songs.cache.json"));
    }

    #[test]
    fn seed_passthrough() {
        let args = Args::try_parse_from(["order_playlist", "--input", "in.csv", "--output", "out.csv", "--seed", "42"]).unwrap();
        let r = args.resolve();
        assert_eq!(r.seed, 42);
        assert!(r.seed_was_supplied);
    }

    #[test]
    fn artist_window_zero_accepted() {
        let args = Args::try_parse_from(["order_playlist", "--input", "in.csv", "--output", "out.csv", "--artist-window", "0"]).unwrap();
        assert_eq!(args.resolve().artist_window, 0);
    }

    #[test]
    fn missing_required_arg_errors() {
        let result = Args::try_parse_from(["order_playlist", "--input", "in.csv"]);
        assert!(result.is_err());
    }
}
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --lib cli::args`
Expected: all pass.

**Commit:** `Phase 7: clap Args with ResolvedArgs default-resolution`
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Wire cli/mod.rs

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/cli/mod.rs`

**Implementation:**

```rust
//! CLI presentation surface — clap args, ASCII chart, summary report.

pub mod args;
pub mod chart;
pub mod report;

pub use args::{Args, ResolvedArgs};
pub use chart::render_arc;
pub use report::format_summary;
```

`chart` and `report` are stubs created in Tasks 3 and 4 — add empty files now so the build doesn't fail:

`src/cli/chart.rs`:
```rust
//! Stub — replaced in Task 3.
```

`src/cli/report.rs`:
```rust
//! Stub — replaced in Task 4.
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo build`
Expected: green.

**Commit:** Bundle with Task 1.
<!-- END_TASK_2 -->

<!-- END_SUBCOMPONENT_A -->

<!-- START_SUBCOMPONENT_B (tasks 3-4) -->

<!-- START_TASK_3 -->
### Task 3: Implement render_arc — ASCII chart of energy vs position

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/cli/chart.rs`

**Implementation:**

```rust
//! ASCII energy-arc chart for stdout.
//!
//! Deterministic output: snapshot-tested via `insta`. Renders the
//! per-position energy of the reordered playlist as a vertical bar
//! chart of fixed width.

use crate::domain::Track;

/// Render an ASCII chart of energy vs position.
///
/// - `tracks[ordering[i]].features.energy` is the value plotted at column `i`.
/// - `width` is the chart height in rows (default 12 in `main.rs`).
///
/// Output format (example, width=12, n=5):
/// ```text
/// Energy arc
/// 1.0 |   #
/// 0.8 |   #
/// 0.6 |  ## #
/// 0.4 | ###  #
/// 0.2 |#####
/// 0.0 +-----
///      12345
/// ```
///
/// Empty input (n=0) renders just the header + axis labels.
pub fn render_arc(tracks: &[Track], ordering: &[usize], width: usize) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    // `write!`/`writeln!` against a `String` cannot fail (the underlying
    // `fmt::Write` impl for `String` is infallible). We use `let _ = ...`
    // throughout to silence the Result without `.unwrap()` — project rule:
    // no `unwrap()` outside tests/main.
    let _ = writeln!(out, "Energy arc");

    if ordering.is_empty() {
        let _ = writeln!(out, "(empty)");
        return out;
    }

    let n = ordering.len();
    let rows = width.max(2);
    // Build matrix top-down: row 0 is highest energy.
    for r in (0..rows).rev() {
        let threshold = (r as f32) / (rows as f32 - 1.0);
        let label = format!("{:.1}", threshold);
        let _ = write!(out, "{label} |");
        for &i in ordering {
            let e = tracks[i].features.energy.get();
            out.push(if e >= threshold { '#' } else { ' ' });
        }
        let _ = writeln!(out);
    }
    // Bottom axis.
    let _ = write!(out, "    +");
    for _ in 0..n { out.push('-'); }
    let _ = writeln!(out);
    // Position labels (1-indexed, single digit; wrap if n > 9).
    let _ = write!(out, "     ");
    for i in 1..=n {
        // unwrap_or('?') is a total function, not an unwrap-on-Err.
        out.push(char::from_digit((i % 10) as u32, 10).unwrap_or('?'));
    }
    let _ = writeln!(out);
    out
}
```

**Testing:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Track, TrackFeatures, TrackId, TrackQuery, Bpm, PitchClass, Mode, Normalized};

    fn track(energy: f32) -> Track {
        Track {
            query: TrackQuery::new("title", "artist"),
            id: TrackId::new("ZZZZ00000000"),
            features: TrackFeatures {
                tempo: Bpm::new(120.0).unwrap(),
                key: PitchClass::new(0).unwrap(),
                mode: Mode::Major,
                energy: Normalized::clamp(energy),
                danceability: Normalized::clamp(0.5),
                valence: Normalized::clamp(0.5),
                loudness: -10.0,
                acousticness: Normalized::clamp(0.5),
                instrumentalness: Normalized::clamp(0.0),
                liveness: Normalized::clamp(0.0),
                speechiness: Normalized::clamp(0.0),
            },
        }
    }

    #[test]
    fn renders_for_n_1() {
        let tracks = vec![track(0.5)];
        let s = render_arc(&tracks, &[0], 6);
        insta::assert_snapshot!("arc_n1", s);
    }

    #[test]
    fn renders_for_n_2() {
        let tracks = vec![track(0.2), track(0.9)];
        let s = render_arc(&tracks, &[0, 1], 6);
        insta::assert_snapshot!("arc_n2", s);
    }

    #[test]
    fn renders_for_n_3() {
        let tracks = vec![track(0.3), track(0.7), track(0.5)];
        let s = render_arc(&tracks, &[0, 1, 2], 6);
        insta::assert_snapshot!("arc_n3", s);
    }

    #[test]
    fn renders_for_n_10_climbing_arc() {
        let tracks: Vec<_> = (0..10).map(|i| track(i as f32 / 10.0)).collect();
        let s = render_arc(&tracks, &(0..10).collect::<Vec<_>>(), 6);
        insta::assert_snapshot!("arc_n10_climbing", s);
    }

    #[test]
    fn empty_renders_without_panic() {
        let s = render_arc(&[], &[], 6);
        assert!(s.contains("Energy arc"));
    }
}
```

First run will fail (no snapshots yet). Run `cargo insta review` to accept the new snapshots, then re-run the test.

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --lib cli::chart`
Expected: snapshot tests pass after `cargo insta accept`.

**Commit:** `Phase 7: ASCII energy-arc chart with insta snapshot tests`
<!-- END_TASK_3 -->

<!-- START_TASK_4 -->
### Task 4: Implement format_summary — resolved/unresolved counts + cost breakdown

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/cli/report.rs`

**Implementation:**

```rust
//! Summary report printed to stdout at end of run.
//!
//! Format (example):
//! ```text
//! Summary
//!   resolved:     38
//!   unresolved:    2  (see unresolved.csv)
//!   seed:         42  (supplied)
//!   total cost:   before  12.345    after  7.891
//!   arc dev:      before   8.000    after  4.123
//!   cost terms (after):
//!     arc        4.123
//!     camelot    1.234
//!     tempo      0.567
//!     energy     0.890
//!     artist     1.077
//!   artist clashes remaining: 0
//! ```

use crate::algo::CostContext;
use crate::domain::Track;
use std::fmt::Write;
use std::path::Path;

pub struct SummaryInputs<'a> {
    pub resolved: usize,
    pub unresolved: usize,
    pub unresolved_path: &'a Path,
    pub seed: u64,
    pub seed_was_supplied: bool,
    pub before_cost: f32,
    pub after_cost: f32,
    pub before_arc_dev: f32,
    pub after_arc_dev: f32,
    pub cost_breakdown: CostBreakdown,
    pub remaining_clashes: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct CostBreakdown {
    pub arc: f32,
    pub camelot: f32,
    pub tempo: f32,
    pub energy: f32,
    pub artist: f32,
}

pub fn format_summary(s: &SummaryInputs<'_>) -> String {
    let mut out = String::new();
    // `write!`/`writeln!` against a `String` is infallible (the
    // underlying `fmt::Write for String` impl cannot fail). We use
    // `let _ = ...` throughout instead of `.unwrap()` — project rule:
    // no `unwrap()` outside tests/main.
    let _ = writeln!(out, "Summary");
    let _ = writeln!(out, "  resolved:    {:>3}", s.resolved);
    if s.unresolved > 0 {
        let _ = writeln!(out, "  unresolved:  {:>3}  (see {})", s.unresolved, s.unresolved_path.display());
    } else {
        let _ = writeln!(out, "  unresolved:  {:>3}", s.unresolved);
    }
    let seed_origin = if s.seed_was_supplied { "supplied" } else { "system-time" };
    let _ = writeln!(out, "  seed:        {} ({})", s.seed, seed_origin);
    let _ = writeln!(out, "  total cost:  before {:>8.3}    after {:>8.3}", s.before_cost, s.after_cost);
    let _ = writeln!(out, "  arc dev:     before {:>8.3}    after {:>8.3}", s.before_arc_dev, s.after_arc_dev);
    let _ = writeln!(out, "  cost terms (after):");
    let _ = writeln!(out, "    arc        {:>8.3}", s.cost_breakdown.arc);
    let _ = writeln!(out, "    camelot    {:>8.3}", s.cost_breakdown.camelot);
    let _ = writeln!(out, "    tempo      {:>8.3}", s.cost_breakdown.tempo);
    let _ = writeln!(out, "    energy     {:>8.3}", s.cost_breakdown.energy);
    let _ = writeln!(out, "    artist     {:>8.3}", s.cost_breakdown.artist);
    let _ = writeln!(out, "  artist clashes remaining: {}", s.remaining_clashes);
    out
}

/// Helper to count artist clashes in an ordering (used by orchestration
/// for the `remaining_clashes` field; verifies AC3.4 reporting).
pub fn count_artist_clashes(tracks: &[Track], ordering: &[usize], window: u8) -> usize {
    if window == 0 { return 0; }
    let w = window as usize;
    let mut count = 0;
    for i in 0..ordering.len() {
        for j in (i+1)..(i + w + 1).min(ordering.len()) {
            if tracks[ordering[i]].query.artist.eq_ignore_ascii_case(&tracks[ordering[j]].query.artist) {
                count += 1;
            }
        }
    }
    count
}
```

**Testing:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn happy_path_summary_snapshot() {
        let s = SummaryInputs {
            resolved: 38, unresolved: 2,
            unresolved_path: &PathBuf::from("unresolved.csv"),
            seed: 42, seed_was_supplied: true,
            before_cost: 12.345, after_cost: 7.891,
            before_arc_dev: 8.0, after_arc_dev: 4.123,
            cost_breakdown: CostBreakdown { arc: 4.123, camelot: 1.234, tempo: 0.567, energy: 0.890, artist: 1.077 },
            remaining_clashes: 0,
        };
        insta::assert_snapshot!("summary_happy", format_summary(&s));
    }

    #[test]
    fn no_unresolved_omits_sidecar_pointer() {
        let s = SummaryInputs {
            resolved: 10, unresolved: 0,
            unresolved_path: &PathBuf::from("unresolved.csv"),
            seed: 0, seed_was_supplied: false,
            before_cost: 5.0, after_cost: 5.0,
            before_arc_dev: 5.0, after_arc_dev: 5.0,
            cost_breakdown: CostBreakdown { arc: 5.0, camelot: 0.0, tempo: 0.0, energy: 0.0, artist: 0.0 },
            remaining_clashes: 0,
        };
        let s = format_summary(&s);
        assert!(!s.contains("see unresolved.csv"));
    }

    #[test]
    fn count_artist_clashes_with_window_4_inclusive() {
        // 5 tracks alternating: A B A B A. Window=4 means any pair (i, j)
        // with `j - i <= 4` (inclusive) and same artist counts as a clash.
        // A-positions are 0, 2, 4: pair (0,2) → distance 2; (0,4) → distance 4
        // (boundary, INCLUSIVE); (2,4) → distance 2. Three clashes.
        let mk = |artist: &str| Track {
            query: crate::domain::TrackQuery::new("t", artist),
            id: crate::domain::TrackId::new("Z"),
            features: crate::domain::TrackFeatures::neutral(),
        };
        let tracks = vec![mk("A"), mk("B"), mk("A"), mk("B"), mk("A")];
        assert_eq!(count_artist_clashes(&tracks, &[0,1,2,3,4], 4), 3);
    }

    #[test]
    fn count_artist_clashes_window_3_excludes_distance_4() {
        // Boundary test: with window=3, (0,4) is OUTSIDE the window and
        // is not counted; only (0,2) and (2,4) remain.
        let mk = |artist: &str| Track {
            query: crate::domain::TrackQuery::new("t", artist),
            id: crate::domain::TrackId::new("Z"),
            features: crate::domain::TrackFeatures::neutral(),
        };
        let tracks = vec![mk("A"), mk("B"), mk("A"), mk("B"), mk("A")];
        assert_eq!(count_artist_clashes(&tracks, &[0,1,2,3,4], 3), 2);
    }
}
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --lib cli::report`
Expected: tests pass after `cargo insta accept`.

**Commit:** `Phase 7: summary report formatter with insta snapshot`
<!-- END_TASK_4 -->

<!-- END_SUBCOMPONENT_B -->

<!-- START_SUBCOMPONENT_C (tasks 4a, 4b, 4c, 5, 6) -->

<!-- START_TASK_4A -->
### Task 4a: Promote `CostContext::cost_breakdown` to Phase 3's algo/cost.rs (cross-phase additive change)

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/algo/cost.rs`

**Implementation:**

Phase 7 needs a per-term cost breakdown for the summary report. Adding it as a method on `CostContext` keeps the per-term logic next to `total_cost`/`delta_cost` (single source of truth). The struct also moves to `algo/cost.rs` so the FCIS boundary is preserved.

```rust
/// Per-term breakdown of the cost over an ordering. Returned by
/// `CostContext::cost_breakdown`. Lives in `algo/cost.rs` (not
/// `cli/report.rs`) so that the algorithm owns the cost taxonomy.
#[derive(Debug, Clone, Copy)]
pub struct CostBreakdown {
    pub arc: f32,
    pub camelot: f32,
    pub tempo: f32,
    pub energy: f32,
    pub artist: f32,
}

impl<'a> CostContext<'a> {
    /// Per-term cost decomposition. Σ of all five terms equals
    /// `total_cost(ordering)` (within f32 rounding).
    pub fn cost_breakdown(&self, ordering: &[usize]) -> CostBreakdown {
        // Re-uses the same per-term helpers as total_cost — the
        // implementation refactors total_cost to call cost_breakdown
        // and sum the result, so there is exactly one place that
        // computes each term.
    }
}
```

The implementation strategy: refactor Phase 3 Task 6's `total_cost` body to:

```rust
pub fn total_cost(&self, ordering: &[usize]) -> f32 {
    let b = self.cost_breakdown(ordering);
    b.arc + b.camelot + b.tempo + b.energy + b.artist
}
```

This guarantees the sum-equals-total invariant by construction.

**Testing (added to phase_03 cost.rs tests):**

```rust
proptest! {
    #[test]
    fn breakdown_sum_equals_total_cost(
        n in 5usize..=20,
        seed in any::<u64>(),
    ) {
        let tracks = synthetic_tracks(n, seed);
        let ctx = CostContext {
            tracks: &tracks, weights: CostWeights::default(),
            arc: EnergyArc, camelot_table: CamelotTable::new(),
        };
        let ordering: Vec<usize> = (0..n).collect();
        let total = ctx.total_cost(&ordering);
        let b = ctx.cost_breakdown(&ordering);
        let sum = b.arc + b.camelot + b.tempo + b.energy + b.artist;
        prop_assert!((total - sum).abs() < 1e-3,
            "sum {} vs total {}", sum, total);
    }
}
```

**Verification:**

Re-run Phase 3's verification gates after the addition:
```bash
cd /Users/y/Apps/music/order_playlist
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib algo::cost
cargo build --release
```
All exit 0. The new proptest runs ≥ 256 cases. `delta_cost_equals_total_cost_diff` from Phase 3 still passes.

**Commit:** `Phase 7 (additive to Phase 3): CostBreakdown + cost_breakdown method on CostContext`
<!-- END_TASK_4A -->

<!-- START_TASK_4B -->
### Task 4b: Promote `Cache::all_resolutions` and `Cache::all_features` to Phase 4's adapters/cache.rs (cross-phase additive change)

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/adapters/cache.rs`

**Implementation:**

Phase 7's integration tests (Task 7) need read iterators over the cache contents to build `InMemoryResolver` / `InMemoryFeatureSource` from a pre-warmed cache fixture. Add these read-only accessors to `Cache`:

```rust
impl Cache {
    /// Iterate over all (TrackQuery, &TrackId) resolution entries.
    /// Used by integration-test scaffolding (Phase 7 Task 7) to
    /// hydrate in-memory adapters from a warm-cache fixture.
    pub fn all_resolutions(&self) -> impl Iterator<Item = (&TrackQuery, &TrackId)> {
        self.resolutions.iter()
    }

    /// Iterate over all (&TrackId, &TrackFeatures) feature entries.
    pub fn all_features(&self) -> impl Iterator<Item = (&TrackId, &TrackFeatures)> {
        self.features.iter()
    }
}
```

**Testing (added to phase_04 cache.rs tests):**

```rust
#[test]
fn all_resolutions_and_all_features_round_trip() {
    let mut cache = Cache::load(std::path::Path::new("/nonexistent")).unwrap();
    let q = TrackQuery::new("Get Lucky", "Daft Punk");
    let id = TrackId::new("USQX91300120");
    cache.put_resolution(q.clone(), id.clone());
    cache.put_features(id.clone(), TrackFeatures::neutral());

    let resolutions: Vec<_> = cache.all_resolutions().collect();
    assert_eq!(resolutions.len(), 1);
    assert_eq!(resolutions[0].0, &q);
    assert_eq!(resolutions[0].1, &id);

    let features: Vec<_> = cache.all_features().collect();
    assert_eq!(features.len(), 1);
}
```

**Verification:**

Re-run Phase 4's verification gates:
```bash
cd /Users/y/Apps/music/order_playlist
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --lib adapters::cache
cargo build --release
```
All exit 0.

**Commit:** `Phase 7 (additive to Phase 4): Cache::all_resolutions + all_features for test scaffolding`
<!-- END_TASK_4B -->

<!-- START_TASK_4C -->
### Task 4c: Wire the moved CostBreakdown re-export in cli/report.rs

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/cli/report.rs`

**Implementation:**

Task 4 (already in this phase) defined `CostBreakdown` in `cli/report.rs`. Task 4a moves the canonical definition to `algo/cost.rs`. Update `cli/report.rs` to re-export it (and remove the local duplicate) so callers can still `use crate::cli::report::CostBreakdown` if they like:

```rust
// At the top of cli/report.rs, after the existing imports:
pub use crate::algo::cost::CostBreakdown;
```

Delete the local `pub struct CostBreakdown { ... }` from `cli/report.rs` (added in Task 4); the re-export replaces it.

**Verification:**

Re-run cli/report tests to ensure the snapshot still matches:
```bash
cd /Users/y/Apps/music/order_playlist
cargo test --lib cli::report
```
Expected: snapshot passes unchanged.

**Commit:** Bundle with Task 4a.
<!-- END_TASK_4C -->

<!-- START_TASK_5 -->
### Task 5: Implement run() function in lib.rs as the orchestration entry point (with cache-partition for AC7.1)

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/lib.rs`
- Create: `/Users/y/Apps/music/order_playlist/src/run.rs`

**Implementation:**

Pulling the orchestration into a `run()` function in the library lets integration tests call it directly (passing custom resolver/feature-source impls) without spawning a subprocess.

**Critical invariant for AC7.1:** when the cache fully covers the input queries, `deps.resolver.resolve_many` and `deps.feature_source.features_for` MUST NOT be called. Even though Phase 5's `MusicBrainzIsrcResolver` does internal cache read-through, the trait-level `Box<dyn Resolver>` may be a `PanicOnCallResolver` that panics on any call. The orchestration must therefore partition queries against the cache BEFORE delegating, and skip the adapter call when the un-cached set is empty.

`src/lib.rs`:
```rust
//! `order_playlist` library crate.

pub mod adapters;
pub mod algo;
pub mod cli;
pub mod domain;
pub mod errors;
pub mod run;

pub use run::{run, ExitCode, RunDeps, RunReport};
```

`src/run.rs`:
```rust
//! Orchestration: read CSV → cache-partition → resolve → fetch features → anneal → write outputs.
//!
//! Exposed to integration tests so the pipeline can be exercised with
//! `InMemoryResolver` + `InMemoryFeatureSource` (or `PanicOnCall*` doubles).

use std::sync::Arc;
use tokio::sync::Mutex;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

use crate::adapters::{Cache, FeatureSource, Resolution, Resolver};
use crate::adapters::{read_input, write_output, write_unresolved, Unresolved};
use crate::algo::{optimize, AnnealConfig, CamelotTable, CostContext, CostWeights, EnergyArc};
use crate::cli::{ResolvedArgs, format_summary, render_arc, report::{SummaryInputs, count_artist_clashes}};
use crate::domain::{Track, TrackId, TrackQuery};

/// Semantic exit codes (per design).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Success = 0,
    Other = 1,
    BadArgs = 2,
    InputError = 3,
    CacheError = 4,
    NothingResolved = 5,
    NetworkExhausted = 6,
}

pub struct RunDeps {
    pub resolver: Box<dyn Resolver>,
    pub feature_source: Box<dyn FeatureSource>,
}

/// Diagnostic payload returned alongside an `ExitCode`. Lets `main.rs`
/// (or test harnesses) format their own user-facing message rather than
/// having `run()` `eprintln!` from inside the library — keeps the
/// CLI/lib separation that `cli/mod.rs` declares.
#[derive(Debug, Clone, Default)]
pub struct RunReport {
    /// Human-readable message attached to a non-success exit (or empty).
    pub message: String,
}

pub async fn run(args: ResolvedArgs, deps: RunDeps) -> Result<(ExitCode, RunReport), miette::Report> {
    // 1. Read input.
    let queries = read_input(&args.input).map_err(miette::Report::new)?;
    tracing::info!(count = queries.len(), input = %args.input.display(), "loaded input");

    // 2. Load cache.
    let cache = Arc::new(Mutex::new(
        Cache::load(&args.cache).map_err(miette::Report::new)?
    ));

    // 3. AC7.1: Partition queries against the cache BEFORE calling the resolver.
    //    Cached "explicit failure" (empty TrackId) → goes straight to `unresolved`.
    //    Cached "success" (non-empty TrackId) → goes straight to `resolved_pairs`.
    //    Anything else → into `to_resolve`, which the resolver sees only when non-empty.
    let mut resolved_pairs: Vec<(TrackQuery, TrackId)> = Vec::new();
    let mut unresolved: Vec<Unresolved> = Vec::new();
    let mut to_resolve: Vec<TrackQuery> = Vec::new();
    {
        let cache_lock = cache.lock().await;
        for q in queries {
            match cache_lock.get_resolution(&q) {
                Some(id) if !id.get().is_empty() => resolved_pairs.push((q, id.clone())),
                Some(_) => {
                    tracing::warn!(title = %q.title, artist = %q.artist, "unresolved: cached prior failure");
                    unresolved.push(Unresolved { query: q, reason: "cached: no ISRC on prior run".into() });
                }
                None => to_resolve.push(q),
            }
        }
    }

    if !to_resolve.is_empty() {
        for r in deps.resolver.resolve_many(&to_resolve).await {
            match r {
                Resolution::Resolved { query, id } => resolved_pairs.push((query, id)),
                Resolution::Unresolved { query, reason } => {
                    tracing::warn!(title = %query.title, artist = %query.artist, %reason, "unresolved");
                    unresolved.push(Unresolved { query, reason });
                }
            }
        }
    }

    // 4. AC7.1: Partition IDs against the feature cache. Same logic, applied
    //    to feature lookup. Only un-cached IDs are passed to `features_for`.
    let mut tracks: Vec<Track> = Vec::new();
    let mut to_fetch: Vec<TrackId> = Vec::new();
    let mut pending: Vec<(TrackQuery, TrackId)> = Vec::new();
    {
        let cache_lock = cache.lock().await;
        for (q, id) in resolved_pairs {
            match cache_lock.get_features(&id) {
                Some(f) => tracks.push(Track { query: q, id, features: f.clone() }),
                None => {
                    pending.push((q, id.clone()));
                    to_fetch.push(id);
                }
            }
        }
    }

    if !to_fetch.is_empty() {
        let fetched = deps.feature_source.features_for(&to_fetch).await;
        // `fetched` is in the same order as `to_fetch`; zip with `pending`.
        for ((q, id), (_, feat)) in pending.into_iter().zip(fetched) {
            match feat {
                Some(f) => tracks.push(Track { query: q, id, features: f }),
                None => {
                    tracing::warn!(title = %q.title, artist = %q.artist, "unresolved: feature lookup returned None");
                    unresolved.push(Unresolved { query: q, reason: "feature lookup returned None".into() });
                }
            }
        }
    }

    // 5. Save cache (best-effort).
    {
        let c = cache.lock().await;
        if let Err(e) = c.save_atomic() {
            tracing::warn!(error = %format!("{e:?}"), "cache save failed; continuing");
        }
    }

    // 6. Always write unresolved.csv when there's content (AC4.1 + AC4.5).
    if !unresolved.is_empty() {
        write_unresolved(&args.unresolved, &unresolved).map_err(miette::Report::new)?;
    }

    // 7. Bail out if nothing resolved (AC4.4). `main.rs` formats the message.
    if tracks.is_empty() {
        return Ok((ExitCode::NothingResolved, RunReport {
            message: "no tracks resolved; nothing to anneal".into(),
        }));
    }

    // 8. Anneal.
    let ctx = CostContext {
        tracks: &tracks,
        weights: CostWeights { artist_window: args.artist_window, ..Default::default() },
        arc: EnergyArc,
        camelot_table: CamelotTable::new(),
    };
    let initial: Vec<usize> = (0..tracks.len()).collect();
    let before_cost = ctx.total_cost(&initial);
    let before_arc_dev = compute_arc_dev(&tracks, &initial);

    let mut rng = ChaCha20Rng::seed_from_u64(args.seed);
    let ordering = optimize(initial, &ctx, &AnnealConfig::default(), &mut rng);
    let after_cost = ctx.total_cost(&ordering);
    let after_arc_dev = compute_arc_dev(&tracks, &ordering);

    // 9. Write output CSV.
    write_output(&args.output, &ordering, &tracks).map_err(miette::Report::new)?;

    // 10. Print chart + summary to stdout. (run.rs is permitted to write
    //     to stdout for the deliverables; only stderr error printing is
    //     forbidden — see RunReport above.)
    print!("{}", render_arc(&tracks, &ordering, 12));
    let breakdown = ctx.cost_breakdown(&ordering);
    let summary = format_summary(&SummaryInputs {
        resolved: tracks.len(),
        unresolved: unresolved.len(),
        unresolved_path: &args.unresolved,
        seed: args.seed,
        seed_was_supplied: args.seed_was_supplied,
        before_cost, after_cost,
        before_arc_dev, after_arc_dev,
        cost_breakdown: breakdown,
        remaining_clashes: count_artist_clashes(&tracks, &ordering, args.artist_window),
    });
    print!("{}", summary);

    Ok((ExitCode::Success, RunReport::default()))
}

fn compute_arc_dev(tracks: &[Track], ordering: &[usize]) -> f32 {
    let arc = EnergyArc;
    let n = ordering.len();
    ordering.iter().enumerate()
        .map(|(i, &idx)| arc.deviation_cost(i, n, tracks[idx].features.energy))
        .sum()
}
```

**No `todo!()`, no `eprintln!` in `run.rs`.** `ctx.cost_breakdown` is the method added by Task 4a. The `NothingResolved` exit returns a `RunReport` whose `message` field `main.rs` writes to stderr — keeps presentation in the CLI layer per `cli/mod.rs`'s "main.rs is the only other place permitted to call println!/eprintln! directly" rule.

**Testing:**

Tested via the integration tests in Tasks 7–10 — `run.rs` is the seam they exercise. No standalone unit tests here. Per Task 4a, `ctx.cost_breakdown` already has its own property test in `algo/cost.rs`.

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo build && cargo test --lib`
Expected: builds and all unit tests pass. The orchestration's correctness is verified by Tasks 7–10's integration tests, which run after this task.

**Commit:** `Phase 7: run() orchestration with cache-partition for AC7.1, RunReport, and ctx.cost_breakdown plumbing`
<!-- END_TASK_5 -->

<!-- START_TASK_6 -->
### Task 6: Implement main.rs — panic hook, tracing-subscriber, deps wiring, exit codes

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/main.rs`

**Implementation:**

```rust
//! `order_playlist` binary entry point.

use clap::Parser;
use order_playlist::adapters::{Cache, FeatureSource, Resolver};
use order_playlist::cli::Args;
use order_playlist::run::{run, ExitCode, RunDeps};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> miette::Result<()> {
    // AC9.3: install miette panic hook for source spans + help text.
    miette::set_panic_hook();

    let args = Args::parse().resolve();
    init_tracing(args.verbose);

    if !args.seed_was_supplied {
        tracing::info!(seed = args.seed, "no --seed supplied; derived from system time");
    }

    let cache_path = args.cache.clone();
    let cache = Arc::new(Mutex::new(
        Cache::load(&cache_path).map_err(miette::Report::new)?
    ));

    let deps = build_deps(&args, cache.clone())?;
    let (exit, report) = run(args, deps).await?;
    if !report.message.is_empty() {
        eprintln!("error: {}", report.message);
    }

    std::process::exit(exit as i32);
}

/// `init_tracing` is `main.rs`-only. Integration tests call `run()`
/// directly and don't install a global subscriber, so they see only
/// the default no-op subscriber — no double-init panic.
fn init_tracing(verbose: u8) {
    use tracing_subscriber::EnvFilter;
    let default_level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("order_playlist={}", default_level)));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

/// Construct the resolver + feature source based on enabled Cargo features.
/// AC8.3: when no provider feature is enabled, emit a clear error.
fn build_deps(args: &order_playlist::cli::ResolvedArgs, cache: Arc<Mutex<Cache>>) -> miette::Result<RunDeps> {
    #[cfg(all(feature = "musicbrainz", feature = "reccobeats"))]
    {
        let resolver = Box::new(
            order_playlist::adapters::MusicBrainzIsrcResolver::new(
                cache.clone(),
                format!("order_playlist/{} ({})", env!("CARGO_PKG_VERSION"), args.musicbrainz_contact),
            ).map_err(|e| miette::miette!("failed to build MusicBrainz client: {e}"))?
        ) as Box<dyn Resolver>;

        let feature_source = Box::new(
            order_playlist::adapters::ReccoBeatsFeatures::new(cache.clone())
                .map_err(|e| miette::miette!("failed to build ReccoBeats client: {e}"))?
        ) as Box<dyn FeatureSource>;

        return Ok(RunDeps { resolver, feature_source });
    }

    #[cfg(not(all(feature = "musicbrainz", feature = "reccobeats")))]
    {
        let _ = (args, cache);
        Err(miette::miette!(
            "no resolver/feature source compiled in; build with `--features musicbrainz,reccobeats`"
        ))
    }
}
```

The `#[cfg(...)]` blocks ensure AC8.3 holds: when both features are absent, the binary compiles but errors immediately with a clear message.

**Testing:**

Manual smoke test (run in shell, not part of `cargo test`):

```bash
cd /Users/y/Apps/music/order_playlist

# Create a tiny 3-song fixture.
mkdir -p data
cat > data/tiny.csv <<EOF
title,artist
Get Lucky,Daft Punk
One More Time,Daft Punk
Around the World,Daft Punk
EOF

# Cold-cache run will hit the network — skip unless you want to wait.
# cargo run --release -- --input data/tiny.csv --output data/tiny.out.csv --seed 42

# Bare-bones build → startup error (AC8.3):
cargo build --release --no-default-features
./target/release/order_playlist --input data/tiny.csv --output data/tiny.out.csv --seed 42
# Expected: exit non-zero with "no resolver/feature source compiled in".
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo build --release`
Expected: green.

Run: `cd /Users/y/Apps/music/order_playlist && cargo build --release --no-default-features`
Expected: green; binary at `target/release/order_playlist` exists.

Run the bare-bones binary:
```bash
cd /Users/y/Apps/music/order_playlist
./target/release/order_playlist --input /tmp/anything.csv --output /tmp/out.csv 2>&1 | head -5
```
Expected: error message containing "no resolver/feature source compiled in".

**Commit:** `Phase 7: main.rs orchestration with miette panic hook, tracing init, AC8.3 startup error`
<!-- END_TASK_6 -->

<!-- END_SUBCOMPONENT_C -->

<!-- START_SUBCOMPONENT_D (tasks 7-10) -->

<!-- START_TASK_7 -->
### Task 7: End-to-end test with in-memory adapters and pre-warmed cache

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/tests/fixtures/small_party.csv`
- Create: `/Users/y/Apps/music/order_playlist/tests/fixtures/small_party.cache.json`
- Create: `/Users/y/Apps/music/order_playlist/tests/end_to_end.rs`

**Implementation:**

`tests/fixtures/small_party.csv` (10 tracks, three artists for the artist-spacing constraint to bite):
```
title,artist
Get Lucky,Daft Punk
One More Time,Daft Punk
Harder Better Faster Stronger,Daft Punk
Around the World,Daft Punk
Take a Chance on Me,ABBA
Dancing Queen,ABBA
Mamma Mia,ABBA
SOS,ABBA
Levitating,Dua Lipa
Don't Start Now,Dua Lipa
```

`tests/fixtures/small_party.cache.json` — a warm cache that maps each of the 10 queries to a synthetic `TrackId` (`FAKE000000001`..`FAKE000000010`) and assigns each ID a synthetic `TrackFeatures`. Generate it via the helper script below — do NOT hand-write JSON for `BTreeMap`-backed structs, since field-order matters for AC5.1 byte-identical determinism.

**Fixture generator** — add a small `[[bin]]` to `Cargo.toml`:
```toml
[[bin]]
name = "build_small_party_cache"
path = "tests/fixtures/build_small_party_cache.rs"
required-features = []
```

Create `tests/fixtures/build_small_party_cache.rs`:
```rust
//! Generate `tests/fixtures/small_party.cache.json` from a fixed seed.
//! Run: `cargo run --bin build_small_party_cache`.
//! The output is committed to the repo so `cargo test` is hermetic.

use std::path::PathBuf;
use order_playlist::adapters::Cache;
use order_playlist::domain::{Bpm, Mode, Normalized, PitchClass, TrackFeatures, TrackId, TrackQuery};

fn main() {
    let queries = [
        ("Get Lucky", "Daft Punk"),
        ("One More Time", "Daft Punk"),
        ("Harder Better Faster Stronger", "Daft Punk"),
        ("Around the World", "Daft Punk"),
        ("Take a Chance on Me", "ABBA"),
        ("Dancing Queen", "ABBA"),
        ("Mamma Mia", "ABBA"),
        ("SOS", "ABBA"),
        ("Levitating", "Dua Lipa"),
        ("Don't Start Now", "Dua Lipa"),
    ];

    let path = PathBuf::from("tests/fixtures/small_party.cache.json");
    // Start fresh — explicitly overwrite the file rather than merging.
    if path.exists() { std::fs::remove_file(&path).unwrap(); }
    let mut cache = Cache::load(&path).unwrap();

    for (i, (title, artist)) in queries.iter().enumerate() {
        let q = TrackQuery::new(*title, *artist);
        let id = TrackId::new(format!("FAKE{:09}", i + 1));
        // Use Bpm/PitchClass/Normalized constructors so values
        // round-trip identically through serde.
        let features = TrackFeatures {
            tempo: Bpm::new(110.0 + 5.0 * (i as f32)).unwrap(),
            key: PitchClass::new((i as u8) % 12).unwrap(),
            mode: if i % 2 == 0 { Mode::Major } else { Mode::Minor },
            energy: Normalized::clamp(0.3 + 0.06 * (i as f32)),
            danceability: Normalized::clamp(0.5),
            valence: Normalized::clamp(0.5),
            loudness: -10.0,
            acousticness: Normalized::clamp(0.2),
            instrumentalness: Normalized::clamp(0.05),
            liveness: Normalized::clamp(0.1),
            speechiness: Normalized::clamp(0.05),
        };
        cache.put_resolution(q, id.clone());
        cache.put_features(id, features);
    }

    cache.save_atomic().unwrap();
    println!("wrote {}", path.display());
}
```

Run once locally and commit the generated `small_party.cache.json`:
```bash
cd /Users/y/Apps/music/order_playlist
cargo run --bin build_small_party_cache
git add tests/fixtures/small_party.cache.json
```

The generator is itself a committed artifact (under `tests/fixtures/`) so the fixture is reproducible from source. Re-running the generator after any `TrackFeatures` schema change regenerates the fixture; the committed JSON is the canonical version.

**Verification of the generator (sanity check before using the fixture):**

After running `cargo run --bin build_small_party_cache`:
```bash
cd /Users/y/Apps/music/order_playlist
test -f tests/fixtures/small_party.cache.json
jq '.version' tests/fixtures/small_party.cache.json  # expect 1
jq '.resolutions | length' tests/fixtures/small_party.cache.json  # expect 10
jq '.features | length' tests/fixtures/small_party.cache.json  # expect 10
```

`tests/end_to_end.rs`:
```rust
mod support;

use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

use order_playlist::adapters::{Cache, Resolution};
use order_playlist::cli::ResolvedArgs;
use order_playlist::run::{run, RunDeps, ExitCode};
use order_playlist::domain::{TrackFeatures, TrackId, TrackQuery};

use support::in_memory::{InMemoryResolver, InMemoryFeatureSource};

#[tokio::test]
async fn end_to_end_warm_cache_produces_output() {
    let dir = TempDir::new().unwrap();
    let input = PathBuf::from("tests/fixtures/small_party.csv");
    let output = dir.path().join("out.csv");
    let unresolved = dir.path().join("unresolved.csv");
    let cache = dir.path().join("cache.json");
    // Copy the pre-warmed cache into the temp dir.
    std::fs::copy("tests/fixtures/small_party.cache.json", &cache).unwrap();

    // Build in-memory deps that mirror the cache contents.
    let resolver = build_resolver_from_cache(&cache).await;
    let feature_source = build_features_from_cache(&cache).await;

    let args = ResolvedArgs {
        input, output: output.clone(), unresolved, cache,
        seed: 42, seed_was_supplied: true,
        artist_window: 4, verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };

    let (exit, _report) = run(args, RunDeps {
        resolver: Box::new(resolver),
        feature_source: Box::new(feature_source),
    }).await.unwrap();

    assert_eq!(exit, ExitCode::Success);

    let out = std::fs::read_to_string(&output).unwrap();
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines[0], "position,title,artist,tempo,key,mode,energy,danceability,valence,loudness,isrc");
    assert_eq!(lines.len(), 11); // header + 10 rows
}

async fn build_resolver_from_cache(cache_path: &std::path::Path) -> InMemoryResolver {
    // Uses the `Cache::all_resolutions` accessor added by Task 4b.
    let cache = Cache::load(cache_path).unwrap();
    InMemoryResolver::new(
        cache.all_resolutions().map(|(q, id)| (q.clone(), id.clone()))
    )
}

async fn build_features_from_cache(cache_path: &std::path::Path) -> InMemoryFeatureSource {
    // Uses the `Cache::all_features` accessor added by Task 4b.
    let cache = Cache::load(cache_path).unwrap();
    InMemoryFeatureSource::new(
        cache.all_features().map(|(id, f)| (id.clone(), f.clone()))
    )
}
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --test end_to_end`
Expected: passes; output CSV has the expected header and 10 data rows.

**Commit:** `Phase 7: end-to-end test against small_party fixture with warm cache`
<!-- END_TASK_7 -->

<!-- START_TASK_8 -->
### Task 8: Determinism test — two runs produce byte-identical output

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/tests/support/common.rs`
- Modify: `/Users/y/Apps/music/order_playlist/tests/support/mod.rs` (add `pub mod common;`)
- Create: `/Users/y/Apps/music/order_playlist/tests/determinism.rs`

**Implementation:**

First add `tests/support/common.rs` with the shared run helper. This eliminates the duplication that drove Task 7's helpers and gives Task 8 a single seed-parameterized entry point.

```rust
//! Shared integration-test plumbing for end_to_end / determinism / unresolved tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

use order_playlist::adapters::Cache;
use order_playlist::cli::ResolvedArgs;
use order_playlist::run::{run, RunDeps, RunReport, ExitCode};

use super::in_memory::{InMemoryResolver, InMemoryFeatureSource};

pub struct SmallPartyRun {
    pub dir: TempDir,
    pub output: PathBuf,
    pub unresolved: PathBuf,
    pub exit: ExitCode,
    pub report: RunReport,
}

/// Run the small_party fixture through the orchestration with the
/// given seed, returning paths to the produced artifacts.
pub async fn run_small_party_with_seed(seed: u64) -> SmallPartyRun {
    run_small_party_with_seed_and_skip(seed, &[]).await
}

/// Same as above, but force the named queries to be unresolved by
/// excluding them from the in-memory resolver map. Used by AC5.2 tests.
pub async fn run_small_party_with_seed_and_skip(seed: u64, skip_titles: &[&str]) -> SmallPartyRun {
    let dir = TempDir::new().unwrap();
    let input = PathBuf::from("tests/fixtures/small_party.csv");
    let output = dir.path().join("out.csv");
    let unresolved = dir.path().join("unresolved.csv");
    let cache_path = dir.path().join("cache.json");
    std::fs::copy("tests/fixtures/small_party.cache.json", &cache_path).unwrap();

    let cache = Cache::load(&cache_path).unwrap();

    let resolver_pairs: Vec<_> = cache.all_resolutions()
        .filter(|(q, _)| !skip_titles.contains(&q.title.as_str()))
        .map(|(q, id)| (q.clone(), id.clone()))
        .collect();
    let resolver = InMemoryResolver::new(resolver_pairs);

    let features = InMemoryFeatureSource::new(
        cache.all_features().map(|(id, f)| (id.clone(), f.clone()))
    );

    let args = ResolvedArgs {
        input, output: output.clone(),
        unresolved: unresolved.clone(),
        cache: cache_path,
        seed, seed_was_supplied: true,
        artist_window: 4, verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };

    let (exit, report) = run(args, RunDeps {
        resolver: Box::new(resolver),
        feature_source: Box::new(features),
    }).await.unwrap();

    SmallPartyRun { dir, output, unresolved, exit, report }
}
```

Update `tests/support/mod.rs`:
```rust
//! Shared helpers for integration tests.

pub mod common;
pub mod in_memory;
```

Now `tests/determinism.rs`:
```rust
mod support;

use support::common::{run_small_party_with_seed, run_small_party_with_seed_and_skip};
use order_playlist::run::ExitCode;

#[tokio::test]
async fn two_runs_same_seed_produce_byte_identical_output() {
    let a = run_small_party_with_seed(42).await;
    let b = run_small_party_with_seed(42).await;
    assert_eq!(a.exit, ExitCode::Success);
    assert_eq!(b.exit, ExitCode::Success);

    let bytes_a = std::fs::read(&a.output).unwrap();
    let bytes_b = std::fs::read(&b.output).unwrap();
    assert_eq!(bytes_a, bytes_b, "AC5.1: outputs must be byte-identical");
}

#[tokio::test]
async fn two_runs_same_seed_produce_byte_identical_unresolved() {
    // Force two queries to be unresolved by hiding them from the resolver map.
    let skip = ["Get Lucky", "Dancing Queen"];
    let a = run_small_party_with_seed_and_skip(42, &skip).await;
    let b = run_small_party_with_seed_and_skip(42, &skip).await;
    assert_eq!(a.exit, ExitCode::Success);

    let bytes_a = std::fs::read(&a.unresolved).unwrap();
    let bytes_b = std::fs::read(&b.unresolved).unwrap();
    assert_eq!(bytes_a, bytes_b, "AC5.2: unresolved.csv must be byte-identical");
}

#[tokio::test]
async fn different_seeds_produce_different_orderings() {
    let a = run_small_party_with_seed(1).await;
    let b = run_small_party_with_seed(2).await;

    let bytes_a = std::fs::read(&a.output).unwrap();
    let bytes_b = std::fs::read(&b.output).unwrap();
    assert_ne!(bytes_a, bytes_b, "AC5.3: different seeds must produce different orderings");
}
```

No `todo!()` anywhere — the helper does the actual work.

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --test determinism`
Expected: three tests pass.

**Commit:** `Phase 7: determinism integration tests (AC5.1, AC5.2, AC5.3)`
<!-- END_TASK_8 -->

<!-- START_TASK_9 -->
### Task 9: Unresolved-tracks test (AC4)

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/tests/fixtures/with_bad_rows.csv`
- Create: `/Users/y/Apps/music/order_playlist/tests/unresolved.rs`

**Implementation:**

`tests/fixtures/with_bad_rows.csv` (5 resolvable + 3 bogus):
```
title,artist
Get Lucky,Daft Punk
Dancing Queen,ABBA
Levitating,Dua Lipa
xyzzy not a song,Nobody Real
asdfasdf,Fake Artist
One More Time,Daft Punk
qwerty,Nobody
SOS,ABBA
```

`tests/unresolved.rs`:
```rust
mod support;

use std::path::PathBuf;
use tempfile::TempDir;

use order_playlist::adapters::{Cache, Resolution, Resolver};
use order_playlist::cli::ResolvedArgs;
use order_playlist::run::{run, RunDeps, ExitCode};
use order_playlist::domain::{TrackFeatures, TrackId, TrackQuery};

use support::in_memory::{InMemoryResolver, InMemoryFeatureSource};

#[tokio::test]
async fn partial_unresolved_exits_zero_and_writes_sidecar() {
    let dir = TempDir::new().unwrap();
    let input = PathBuf::from("tests/fixtures/with_bad_rows.csv");
    let unresolved_path = dir.path().join("unresolved.csv");

    // Resolver knows 5 of the 8 queries; the other 3 → Unresolved.
    let resolver = InMemoryResolver::new([
        (TrackQuery::new("Get Lucky", "Daft Punk"), TrackId::new("FAKE001")),
        (TrackQuery::new("Dancing Queen", "ABBA"), TrackId::new("FAKE002")),
        (TrackQuery::new("Levitating", "Dua Lipa"), TrackId::new("FAKE003")),
        (TrackQuery::new("One More Time", "Daft Punk"), TrackId::new("FAKE004")),
        (TrackQuery::new("SOS", "ABBA"), TrackId::new("FAKE005")),
    ]);
    let features = InMemoryFeatureSource::new([
        (TrackId::new("FAKE001"), TrackFeatures::neutral()),
        (TrackId::new("FAKE002"), TrackFeatures::neutral()),
        (TrackId::new("FAKE003"), TrackFeatures::neutral()),
        (TrackId::new("FAKE004"), TrackFeatures::neutral()),
        (TrackId::new("FAKE005"), TrackFeatures::neutral()),
    ]);

    let args = ResolvedArgs {
        input, output: dir.path().join("out.csv"),
        unresolved: unresolved_path.clone(),
        cache: dir.path().join("cache.json"),
        seed: 42, seed_was_supplied: true,
        artist_window: 4, verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };
    let (exit, _report) = run(args, RunDeps {
        resolver: Box::new(resolver),
        feature_source: Box::new(features),
    }).await.unwrap();

    // AC4.2: ≥ 1 resolved → exit 0 even with unresolved.
    assert_eq!(exit, ExitCode::Success);

    // AC4.1: sidecar exists with title,artist,reason columns.
    let sidecar = std::fs::read_to_string(&unresolved_path).unwrap();
    assert!(sidecar.starts_with("title,artist,reason\n"));
    assert_eq!(sidecar.lines().count(), 4); // header + 3 unresolved

    // AC4.5: re-feed the sidecar as input — read_input should accept it.
    let queries = order_playlist::adapters::read_input(&unresolved_path).unwrap();
    assert_eq!(queries.len(), 3);
}

#[tokio::test]
async fn all_unresolved_exits_5() {
    let dir = TempDir::new().unwrap();
    let input = PathBuf::from("tests/fixtures/small_party.csv");

    // Resolver knows nothing.
    let resolver = InMemoryResolver::new([]);
    let features = InMemoryFeatureSource::new([]);

    let args = ResolvedArgs {
        input, output: dir.path().join("out.csv"),
        unresolved: dir.path().join("unresolved.csv"),
        cache: dir.path().join("cache.json"),
        seed: 42, seed_was_supplied: true,
        artist_window: 4, verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };
    let (exit, _report) = run(args, RunDeps {
        resolver: Box::new(resolver),
        feature_source: Box::new(features),
    }).await.unwrap();

    // AC4.4: all unresolved → exit 5.
    assert_eq!(exit, ExitCode::NothingResolved);
}
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --test unresolved`
Expected: both tests pass.

**Commit:** `Phase 7: unresolved sidecar + exit-5 integration tests (AC4)`
<!-- END_TASK_9 -->

<!-- START_TASK_10 -->
### Task 10: Zero-network test with PanicOnCall* doubles (AC7.1)

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/tests/zero_network.rs`

**Implementation:**

```rust
mod support;

use std::path::PathBuf;
use tempfile::TempDir;

use order_playlist::cli::ResolvedArgs;
use order_playlist::run::{run, RunDeps, ExitCode};

use support::in_memory::{PanicOnCallResolver, PanicOnCallFeatureSource};

#[tokio::test]
async fn warm_cache_does_not_invoke_adapters() {
    let dir = TempDir::new().unwrap();
    let input = PathBuf::from("tests/fixtures/small_party.csv");
    let cache = dir.path().join("cache.json");
    // Copy the pre-warmed cache from Task 7's fixture.
    std::fs::copy("tests/fixtures/small_party.cache.json", &cache).unwrap();

    let args = ResolvedArgs {
        input, output: dir.path().join("out.csv"),
        unresolved: dir.path().join("unresolved.csv"),
        cache,
        seed: 42, seed_was_supplied: true,
        artist_window: 4, verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };

    // AC7.1: the panic-on-call adapters MUST NOT be invoked.
    // If `run` reaches into the resolver or feature_source,
    // the panic fails the test with a clear message.
    let (exit, _report) = run(args, RunDeps {
        resolver: Box::new(PanicOnCallResolver),
        feature_source: Box::new(PanicOnCallFeatureSource),
    }).await.unwrap();

    assert_eq!(exit, ExitCode::Success);
}
```

**The critical orchestration property:** the `run` function's cache read-through happens *before* invoking the resolver and feature source — this is the cache-partition built into Task 5. The `PanicOnCall*` adapters here verify the property end-to-end. If Task 5's partition logic is broken, this test fails loudly.

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --test zero_network`
Expected: passes. The `PanicOnCall*` doubles never panic, proving the warm-cache path skips adapter calls entirely.

Run the full integration suite after this task to make sure nothing regressed:
```bash
cd /Users/y/Apps/music/order_playlist
cargo test --test determinism --test end_to_end --test unresolved --test zero_network
```
Expected: all pass.

**Commit:** `Phase 7: zero-network warm-cache integration test (AC7.1 verification)`
<!-- END_TASK_10 -->

<!-- END_SUBCOMPONENT_D -->

<!-- START_SUBCOMPONENT_E (task 11) -->

<!-- START_TASK_11 -->
### Task 11: CI workflow + final verification + commit

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/.github/workflows/ci.yml`

**Implementation:**

```yaml
name: CI

on:
  push:
    branches: [main, playlist-arc-v1]
  pull_request:
    branches: [main]

jobs:
  build-and-test:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        features:
          - "default"
          - "--no-default-features --features musicbrainz,reccobeats"
          - "--no-default-features --features musicbrainz"
          - "--no-default-features --features reccobeats"
          - "--no-default-features"
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: fmt
        run: cargo fmt --check
        if: matrix.features == 'default'

      - name: clippy
        run: cargo clippy --all-targets ${{ matrix.features == 'default' && '' || matrix.features }} -- -D warnings
        if: matrix.features != '--no-default-features'  # bare-bones has no targets to lint here

      - name: build
        run: cargo build ${{ matrix.features == 'default' && '' || matrix.features }}

      - name: test
        run: cargo test ${{ matrix.features == 'default' && '' || matrix.features }}
        if: matrix.features != '--no-default-features'  # no tests without adapters
```

The matrix entry `"--no-default-features"` proves AC8.3 — the build succeeds; the binary errors at startup but that's not a CI concern.

Final verification (run locally before pushing):

```bash
cd /Users/y/Apps/music/order_playlist
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features --features musicbrainz,reccobeats -- -D warnings
cargo test
cargo build --release
cargo build --release --no-default-features
cargo build --release --no-default-features --features musicbrainz,reccobeats
```

All seven exit 0.

Hermetic test gate (AC9.1):
```bash
cd /Users/y/Apps/music/order_playlist
# Run tests with network blocked at the OS level — they should all pass.
# On macOS, the easiest proxy is to check that no test imports `reqwest`
# in a non-mocked code path. The wiremock tests use reqwest but only
# against localhost, which doesn't count as "network".
cargo test 2>&1 | grep -E '(running|passed|failed)' | tail -5
```
Expected: all tests pass, none gate behind `LIVE_NETWORK` env var.

**Commit:** Final:
```bash
cd /Users/y/Apps/music/order_playlist
git add .github/ src/ tests/ Cargo.toml Cargo.lock
git status
git commit -m "Phase 7: CI matrix + final verification suite"
```
<!-- END_TASK_11 -->

<!-- END_SUBCOMPONENT_E -->

---

## Phase 7 Done When

- All integration tests pass: `end_to_end`, `determinism`, `unresolved`, `zero_network`.
- `cargo test` is hermetic (no network).
- `cargo test --features live-network` runs the live-API smoke tests (when network available).
- ASCII chart + summary report snapshots committed via `cargo insta accept`.
- `main.rs` installs the miette panic hook and tracing-subscriber; AC9.3 verified by manual `panic!` test if desired.
- `cargo build --no-default-features --features musicbrainz,reccobeats` succeeds (AC8.1).
- `cargo build --no-default-features` succeeds AND the resulting binary errors clearly at startup (AC8.3).
- CI matrix builds + tests every documented feature combination.
- Manual `cargo run -- --input data/tiny.csv --output data/tiny.out.csv --seed 42` produces a visible arc + summary on stdout.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --release` all exit 0.
- Commits on `playlist-arc-v1` whose subjects start with `Phase 7:`.

## Risk callouts

- **Cross-phase `cost_breakdown` addition.** Task 5 adds a new `pub fn cost_breakdown` to `algo/cost.rs`. This is an additive change to Phase 3; ensure the new method has a unit test confirming `sum(breakdown.*) == total_cost(ordering)` to maintain Phase 3's correctness invariants.
- **`Cache::all_resolutions`/`all_features` additive change.** Task 7 needs read-iterator accessors on `Cache`. These are additive; document in the commit message that they were added for test scaffolding and are not part of the production read path.
- **AC7.1 partition fix.** Task 10's warm-cache test reveals a hole in the Task 5 orchestration — without the explicit partition, the resolver/feature source is invoked even when the cache has everything. The fix is non-trivial; budget time for it.
- **AC9.1 enforcement.** "Hermetic by default" is hard to *prove*; the integration tests use in-memory doubles and the live-network feature is `cfg`-excluded. The CI matrix runs `cargo test` without `live-network`, so any silent network call by an adapter would surface as a CI flake (or a panic if the network is blocked).
- **insta snapshots in CI.** First CI run may fail because the snapshots aren't accepted. Run `cargo insta review` locally and commit the `.snap` files in `tests/snapshots/`. Configure CI to fail on unaccepted snapshots (`INSTA_UPDATE=no`, the default).
- **`itertools` dep for partition.** If you don't want a new dep just for `partition_map`, hand-roll it with two `Vec::with_capacity` and a single `for` loop.
