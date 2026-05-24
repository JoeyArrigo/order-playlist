# Playlist Arc v1 — Phase 3: Pure Algorithm Core

**Goal:** Implement the cost function, energy arc, Camelot distance, and simulated-annealing loop as pure synchronous Rust. Delta-cost is mandatory; full re-evaluation per iteration is a bug, not an optimization opportunity.

**Architecture:** Everything in `src/algo/` is pure: no `tokio`, no IO, no adapter imports, no `anyhow::Result`. The annealer takes borrows of `&[Track]` and a seeded `rand::Rng`, returns a `Vec<usize>` permutation. The artist-spacing hard constraint is encoded as a high-weight pairwise term in `CostWeights` — SA naturally avoids those neighborhoods, no two-tier feasibility logic.

**Tech Stack:** Rust, `rand` + `rand_chacha` (seeded determinism), proptest (algebraic property tests), `tracing` (info on optimization progress, no warn in pure code).

**Scope:** Phase 3 of 7 from `/Users/y/Apps/music/order_playlist/docs/design-plans/2026-05-23-playlist-arc-v1.md`.

**Codebase verified:** 2026-05-23 — Phase 2's domain types will exist by the time this phase starts (`Bpm`, `PitchClass`, `Mode`, `Normalized`, `Track`, `TrackFeatures`, `CamelotCode`). `src/algo/mod.rs` exists as a stub; submodules do not.

**Project guidance:** `/Users/y/Apps/music/order_playlist/implementation-plan-guidance.md`. Key rules for this phase (BLOCKING in code review):
- "Any annealing iteration that does a full cost recompute blocks the merge."
- "Missing unit tests on `cost.rs`, `camelot.rs`, or `anneal.rs` block the merge."
- "Pure modules (`cost.rs`, `camelot.rs`, `anneal.rs`) must have ≥80% line coverage."
- "Delta-cost must be tested against full-recompute" (property test).
- "Determinism test required" (same seed → identical output).

---

## Acceptance Criteria Coverage

This phase implements and tests:

### playlist-arc-v1.AC3: No two songs share an artist within `--artist-window` positions
- **playlist-arc-v1.AC3.5 Verification:** Property test asserts the invariant over random inputs with default window.

(AC3.1–AC3.4 are end-to-end behaviors verified in Phase 7; this phase establishes the cost-function machinery that *makes* them hold.)

### playlist-arc-v1.AC5: Deterministic runs with `--seed`
- **playlist-arc-v1.AC5.3 Success:** Different seeds with the same input produce different orderings (non-trivial search space).

(AC5.1, AC5.2, AC5.4 are CLI-level behaviors verified in Phase 7; this phase ensures `optimize()` itself is bit-deterministic given a seeded `ChaCha20Rng`.)

---

## Task Overview

```
SUBCOMPONENT_A: Camelot distance table (tasks 1-2)
SUBCOMPONENT_B: Energy arc curve (tasks 3-4)
SUBCOMPONENT_C: Cost function (total + delta) (tasks 5-7)
SUBCOMPONENT_D: Simulated annealing loop (tasks 8-9)
SUBCOMPONENT_E: Re-exports, full verification, commit (task 10)
```

---

<!-- START_SUBCOMPONENT_A (tasks 1-2) -->

<!-- START_TASK_1 -->
### Task 1: Implement CamelotTable (24×24 harmonic distance lookup)

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/src/algo/camelot.rs`

**Implementation:**

The Camelot wheel has 12 positions on each of the two letter rings (A = minor, B = major). Harmonic distance follows the DJ practitioner convention:

- **0**: same code (e.g., 8A → 8A).
- **1**: adjacent on the same ring (8A → 9A, 8A → 7A) OR relative flip (8A → 8B).
- **2**: ±2 positions on the same ring (8A → 10A, 8A → 6A), OR adjacent on the *other* ring (8A → 9B, 8A → 7B).
- **4**: ≥ 3 positions apart, on either ring.

Distance wraps at 12 (`(8A → 1A)` is 5, not 7, because the ring is circular — but capped at 4 per the rule above, so realistically only ±1 and ±2 are < 4).

Build a 24×24 `f32` table at initialization. **Index convention (set in Phase 2's `CamelotCode::index()`):** B-ring (major) → 0..=11; A-ring (minor) → 12..=23. Specifically `1B → 0, ..., 12B → 11, 1A → 12, ..., 12A → 23`. The `CamelotTable` builder must reverse this mapping when computing distances: a row/column index in `0..=11` is on the B ring; `12..=23` is on the A ring.

```rust
//! 24×24 harmonic-distance lookup over Camelot codes.
//!
//! Distances follow the DJ practitioner convention:
//! - 0: same code
//! - 1: adjacent on the same ring or relative-flip
//! - 2: ±2 on same ring, or adjacent on the other ring
//! - 4: anything further
//!
//! The table is symmetric (distance is a metric: d(a,b) == d(b,a)).

use crate::domain::CamelotCode;

pub struct CamelotTable {
    distances: [[f32; 24]; 24],
}

impl CamelotTable {
    pub fn new() -> Self { /* build from the rules above */ }

    pub fn distance(&self, a: CamelotCode, b: CamelotCode) -> f32 {
        self.distances[a.index() as usize][b.index() as usize]
    }
}

impl Default for CamelotTable {
    fn default() -> Self { Self::new() }
}
```

**Testing:**

- **Symmetry property** (proptest, exhaustive over the 24×24 grid): `table.distance(a, b) == table.distance(b, a)` for all 576 pairs.
- **Identity**: `table.distance(c, c) == 0.0` for all 24 codes.
- **Hand-checked anchors** (parameterized):
  - `(8A, 8A) → 0`
  - `(8A, 9A) → 1` (adjacent same ring)
  - `(8A, 8B) → 1` (relative flip)
  - `(8A, 10A) → 2` (±2 same ring)
  - `(8A, 9B) → 2` (adjacent other ring)
  - `(8A, 1A) → 4` (far, despite wrap)
  - `(1A, 12A) → 1` (wrap-around adjacency)
- **Coverage**: every cell in the 576-entry table is one of `{0.0, 1.0, 2.0, 4.0}` (loop assertion).

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --lib algo::camelot`
Expected: all unit + property tests pass.

Run a coverage check (optional but recommended for this file per project guidance):
```bash
cd /Users/y/Apps/music/order_playlist
# If cargo-tarpaulin is installed:
cargo tarpaulin --lib --packages order_playlist --include-files 'src/algo/camelot.rs' 2>&1 | tail -5
```
Expected ≥ 80% line coverage. If `cargo tarpaulin` is not installed, document the gap — coverage isn't an automated gate in v1, but the project rule flags it.

**Commit:** `Phase 3: CamelotTable 24x24 harmonic distance lookup`
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Wire algo::camelot into algo/mod.rs

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/algo/mod.rs`

**Implementation:**

```rust
//! Pure algorithm core. **FCIS rule:** zero IO, zero async, zero adapter imports.

pub mod camelot;

pub use camelot::CamelotTable;
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo build && cargo test --lib`
Expected: green.

Pure-core check:
```bash
cd /Users/y/Apps/music/order_playlist
grep -rE "use (std::fs|std::io|tokio|reqwest|crate::adapters|crate::cli|anyhow)" src/algo/
```
Expected: zero matches. (Note `anyhow` is added to the grep — the design says "no `anyhow::Result` in this module tree.")

**Commit:** Bundled with Task 1.
<!-- END_TASK_2 -->

<!-- END_SUBCOMPONENT_A -->

<!-- START_SUBCOMPONENT_B (tasks 3-4) -->

<!-- START_TASK_3 -->
### Task 3: Implement EnergyArc with fixed asymmetric beta-like curve peaking near position 0.68

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/src/algo/arc.rs`

**Implementation:**

The energy arc is a per-position target curve the playlist is shaped to follow. The design specifies "a fixed asymmetric beta-like shape peaking near position 0.68." A beta distribution PDF with `α = 4, β = 2` peaks at `(α-1) / (α+β-2) = 3/4 = 0.75` — close enough for v1 with a small tuning. A simpler computable approximation that peaks near 0.68 and stays smooth:

```
target(t) = 4 * t^2 * (1 - t)    // peaks at t = 2/3 ≈ 0.667
```

normalized to `[0, 1]`: divide by the max (which is `4 * (2/3)^2 * (1/3) = 16/27 ≈ 0.5926`), giving a clean unit-range curve.

```rust
//! Target per-position energy curve.
//!
//! The default curve is a fixed asymmetric shape that peaks near
//! position 0.68 and tapers to ~0.3 at both ends. It is intentionally
//! parameter-free in v1 — the design plan removed anchor/banger
//! pacing knobs to keep v1 small.

use crate::domain::Normalized;

pub struct EnergyArc;

impl EnergyArc {
    /// Target energy in [0, 1] for position `i` out of `n` total tracks.
    /// `n == 0` or `n == 1` returns the curve's peak (0.68) — there is
    /// no meaningful arc with one track, so we surface the peak as a
    /// degenerate fallback that keeps the chart rendering well.
    pub fn target(&self, position: usize, n: usize) -> Normalized { /* ... */ }

    /// Squared-error deviation between `actual` and `target(position, n)`.
    /// The cost function uses this as a per-position term.
    pub fn deviation_cost(&self, position: usize, n: usize, actual: Normalized) -> f32 { /* ... */ }
}

impl Default for EnergyArc {
    fn default() -> Self { EnergyArc }
}
```

The math:
```rust
fn raw_target(t: f32) -> f32 {
    // 4*t^2*(1-t), scaled so max == 1.
    let raw = 4.0 * t * t * (1.0 - t);
    const SCALE: f32 = 27.0 / 16.0;  // 1.0 / (16/27)
    (raw * SCALE).clamp(0.0, 1.0)
}
```

For `n` tracks, position `i` (0-indexed): `t = (i as f32 + 0.5) / n as f32` (midpoint convention — avoids 0.0 and 1.0 endpoints exactly).

**Testing:**

- `target(0, 1).get()` is close to `raw_target(0.5)` ≈ 0.844 (n=1, midpoint at 0.5). Assert `> 0.8`.
- `target(0, 10)` (first of 10) is small (< 0.2).
- `target(7, 10)` is at or near the peak (≥ 0.95 — the integer position closest to 0.68 of n=10 is index 6 or 7).
- `target(9, 10)` (last of 10) tapers (< 0.5).
- `deviation_cost(i, n, x)` is always ≥ 0, and `== 0` when `x == target(i, n)` (within `f32` tolerance — use `f32::EPSILON * 10` as tolerance).
- **Property test**: for any `n ∈ 1..=200` and any `i ∈ 0..n`, `target(i, n)` is in `[0, 1]`.

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --lib algo::arc`
Expected: all tests pass.

**Commit:** `Phase 3: EnergyArc target curve with squared-deviation cost`
<!-- END_TASK_3 -->

<!-- START_TASK_4 -->
### Task 4: Add EnergyArc re-export

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/algo/mod.rs`

**Implementation:**

```rust
pub mod arc;
pub mod camelot;

pub use arc::EnergyArc;
pub use camelot::CamelotTable;
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo build && cargo test --lib algo`
Expected: green.

**Commit:** Bundle with Task 3.
<!-- END_TASK_4 -->

<!-- END_SUBCOMPONENT_B -->

<!-- START_SUBCOMPONENT_C (tasks 5-7) -->

<!-- START_TASK_5 -->
### Task 5: Implement CostWeights and CostContext

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/src/algo/cost.rs`

**Implementation:**

```rust
//! Weighted cost function over a playlist ordering.
//!
//! The cost is a sum of:
//! 1. Per-position arc deviation (EnergyArc).
//! 2. Pairwise Camelot distance over adjacent and near-adjacent tracks.
//! 3. Tempo delta between adjacent tracks.
//! 4. Energy jump between adjacent tracks.
//! 5. Artist-clash penalty within a window — a high-weight term that
//!    encodes the artist-spacing hard constraint as a soft penalty
//!    large enough that SA naturally avoids it.

use crate::algo::{CamelotTable, EnergyArc};
use crate::domain::{CamelotCode, Track};

#[derive(Debug, Clone)]
pub struct CostWeights {
    pub arc_deviation: f32,
    pub camelot_distance: f32,
    pub tempo_delta: f32,
    pub energy_jump: f32,
    pub artist_clash: f32,
    /// Window over which the artist-clash term applies (default 4).
    /// `0` disables the term entirely.
    pub artist_window: u8,
}

impl Default for CostWeights {
    fn default() -> Self {
        Self {
            arc_deviation: 1.0,
            camelot_distance: 0.3,
            tempo_delta: 0.02,
            energy_jump: 0.5,
            // Heavy weight so SA naturally pushes clashes out of view.
            // Must be at least 10× the arc term to dominate at SA's
            // typical temperatures.
            artist_clash: 50.0,
            artist_window: 4,
        }
    }
}

pub struct CostContext<'a> {
    pub tracks: &'a [Track],
    pub weights: CostWeights,
    pub arc: EnergyArc,
    pub camelot_table: CamelotTable,
}
```

**Testing:**

- `CostWeights::default()` has the documented values; explicit unit test prevents silent drift.
- `CostWeights { artist_window: 0, .. default() }` is valid (used by `--artist-window=0` per AC3.2).
- A `CostContext` can be constructed from a `&[Track]` slice (compile-time test, plus runtime construction smoke test).

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --lib algo::cost::tests`
Expected: green.

**Commit:** Bundle with Task 6.
<!-- END_TASK_5 -->

<!-- START_TASK_6 -->
### Task 6: Implement total_cost and delta_cost

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/algo/cost.rs`

**Implementation:**

`total_cost` is a full sweep. `delta_cost` is the **incremental** cost change for a single 2-swap and must only touch the terms affected by the swap.

```rust
impl<'a> CostContext<'a> {
    /// Full cost over `ordering[..]`. Always equal to the sum of:
    ///   - Σ_i  arc.deviation_cost(i, n, tracks[ordering[i]].features.energy)
    ///   - Σ_{i<j, j-i ∈ {1, 2}}  weights * pairwise_term(tracks[ordering[i]], tracks[ordering[j]])
    ///   - Σ_{i<j, j-i ≤ artist_window}  artist_clash if same artist (case-insensitive)
    pub fn total_cost(&self, ordering: &[usize]) -> f32 { /* ... */ }

    /// Incremental cost change for swapping `ordering[a]` and `ordering[b]`
    /// (a != b). Must NOT iterate the full ordering. Only the terms
    /// touching positions `a`, `b`, and their neighbors at distance
    /// 1, 2, and within `artist_window` need re-computation.
    ///
    /// Returns Δ such that:
    ///   total_cost(ordering_after_swap) == total_cost(ordering_before_swap) + Δ
    /// (subject to f32 rounding; tested with a tolerance of 1e-3).
    pub fn delta_cost(&self, ordering: &[usize], a: usize, b: usize) -> f32 { /* ... */ }
}
```

**Pairwise term** (between tracks at adjacent positions `i, i+1`):
```
pairwise = weights.camelot_distance * camelot_table.distance(camelot(track_i), camelot(track_j))
         + weights.tempo_delta * (track_i.tempo - track_j.tempo).abs()
         + weights.energy_jump * (track_i.energy - track_j.energy).abs()
```

For distance-2 pairs (`j - i == 2`), use the same formula but scaled by `0.5` (the design's "four pairwise terms touched by a 2-swap" implies window-of-2 for non-artist pairwise costs).

**Artist clash** is a binary term per pair within `artist_window`:
```
if i < j && j - i <= artist_window
   && tracks[ordering[i]].query.artist.eq_ignore_ascii_case(&tracks[ordering[j]].query.artist)
{
   cost += weights.artist_clash
}
```

The case-insensitive comparison is the project's AC3.1 spec ("case-insensitive exact match").

**Algorithm subtlety for delta_cost.** A 2-swap of positions `a` and `b` (assume `a < b`) affects:
- All pairwise terms involving position `a` or `b` (i.e., `(a-2, a-1, a, a+1, a+2, b-2, b-1, b, b+1, b+2)` overlaps).
- Two arc terms: `arc.deviation_cost(a, n, ...)` and `arc.deviation_cost(b, n, ...)`.
- All artist-window terms involving position `a` or `b`.

Compute the "before" subtotal of just those terms, perform the swap, compute the "after" subtotal of the same terms, return `after - before`. **Do not call `total_cost`** as a shortcut — the whole point is to avoid the O(n) sweep.

**Testing (the critical property test):**

```rust
proptest! {
    #[test]
    fn delta_cost_equals_total_cost_diff(
        n in 5usize..=30,
        seed in any::<u64>(),
    ) {
        let tracks: Vec<Track> = synthetic_tracks(n, seed);
        let ctx = CostContext {
            tracks: &tracks,
            weights: CostWeights::default(),
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };
        let mut ordering: Vec<usize> = (0..n).collect();
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        // Pick a random 2-swap.
        let a = rng.gen_range(0..n);
        let b = loop { let x = rng.gen_range(0..n); if x != a { break x } };

        let before = ctx.total_cost(&ordering);
        let delta = ctx.delta_cost(&ordering, a, b);
        ordering.swap(a, b);
        let after = ctx.total_cost(&ordering);

        prop_assert!((after - (before + delta)).abs() < 1e-3,
            "delta {} vs actual diff {}", delta, after - before);
    }
}
```

The `synthetic_tracks(n, seed)` helper lives in a `#[cfg(test)] mod test_support;` submodule of `cost.rs`. It constructs `n` deterministic `Track`s with seeded random features. Use `TrackFeatures::neutral()` from Phase 2 as the baseline, perturbed slightly per track using the seeded RNG.

Additional unit tests:
- `total_cost(&[0])` (single track) returns just the arc deviation term, with no pairwise or artist contribution.
- `total_cost` of an ordering with two same-artist tracks adjacent (window=4) is at least `weights.artist_clash` higher than the same ordering with them separated by 5+ positions.
- `delta_cost(ordering, a, b) == delta_cost(ordering, b, a)` (swap is symmetric).
- `delta_cost(ordering, a, a)` returns `0.0` (no-op swap).

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --lib algo::cost`
Expected: All tests pass, including ≥ 256 proptest cases for the delta-cost property.

**Commit:** `Phase 3: cost function with mandatory delta-cost (property-tested)`
<!-- END_TASK_6 -->

<!-- START_TASK_7 -->
### Task 7: Wire algo::cost into algo/mod.rs

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/algo/mod.rs`

**Implementation:**

```rust
pub mod arc;
pub mod camelot;
pub mod cost;

pub use arc::EnergyArc;
pub use camelot::CamelotTable;
pub use cost::{CostContext, CostWeights};
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo build && cargo test --lib algo`
Expected: green.

**Commit:** Bundle with Task 6.
<!-- END_TASK_7 -->

<!-- END_SUBCOMPONENT_C -->

<!-- START_SUBCOMPONENT_D (tasks 8-9) -->

<!-- START_TASK_8 -->
### Task 8: Implement AnnealConfig and optimize() with seeded RNG

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/src/algo/anneal.rs`

**Implementation:**

```rust
//! Simulated-annealing loop over playlist permutations.
//!
//! The loop is pure: takes a seeded `R: Rng`, returns a permutation.
//! No tokio, no IO, no logging at iteration granularity (one info!
//! event at start and end is acceptable; per-iteration logging is not).

use rand::Rng;
use crate::algo::cost::CostContext;

#[derive(Debug, Clone)]
pub struct AnnealConfig {
    /// Cooling rate. α < 1; default 0.998.
    pub alpha: f32,
    /// Total iterations across all restarts. Default 100_000.
    pub iterations: usize,
    /// Number of independent restarts; best ordering across restarts is
    /// returned. Default 2.
    pub restarts: u8,
    /// Pilot-calibration iterations (used to set T₀ — see below).
    /// Default 500.
    pub pilot_iterations: usize,
    /// Pilot-calibration target acceptance rate; T₀ is chosen so this
    /// fraction of worsening moves would be accepted at start.
    /// Default 0.4.
    pub pilot_target_acceptance: f32,
}

impl Default for AnnealConfig {
    fn default() -> Self {
        Self {
            alpha: 0.998,
            iterations: 100_000,
            restarts: 2,
            pilot_iterations: 500,
            pilot_target_acceptance: 0.4,
        }
    }
}

/// Run simulated annealing with delta-cost. Returns the best permutation found.
///
/// Determinism contract: given identical `initial`, identical `&[Track]` in
/// `ctx`, identical `AnnealConfig`, and the same `R` state, repeated calls
/// produce identical output. Use `ChaCha20Rng::seed_from_u64(seed)` to get
/// a reproducible `R`.
pub fn optimize<R: Rng>(
    initial: Vec<usize>,
    ctx: &CostContext<'_>,
    config: &AnnealConfig,
    rng: &mut R,
) -> Vec<usize> {
    // 1. Pilot calibration: sample `pilot_iterations` random 2-swaps,
    //    record positive delta_costs, choose T₀ such that
    //    exp(-mean_positive_delta / T₀) ≈ pilot_target_acceptance.
    //    Concretely: T₀ = -mean_positive_delta / ln(pilot_target_acceptance).
    //
    // 2. For each restart r in 0..restarts:
    //      a. Start from a permutation derived from `initial` and the RNG
    //         (shuffle on r > 0).
    //      b. Geometric cooling: T = T₀ * α^iter.
    //      c. For each iteration in 0..(iterations / restarts):
    //          - Pick random a, b in 0..n.
    //          - delta = ctx.delta_cost(ordering, a, b).
    //          - Accept if delta < 0 OR rng.gen::<f32>() < (-delta / T).exp().
    //          - On accept: ordering.swap(a, b); current_cost += delta.
    //          - Track best ordering seen across all restarts.
    //
    // 3. Return the best ordering observed.
    //
    // Performance note (per project guidance): never call total_cost
    // inside the inner loop. Only call total_cost once per restart at
    // the start; subsequently maintain `current_cost` incrementally
    // via `delta_cost`.
}
```

Use `tracing::info!` exactly **twice** in this function: once at start (with the calibrated T₀ and config), once at end (with iterations completed, restarts done, final best cost, initial cost). No per-iteration logging — that would slow the inner loop by orders of magnitude.

**Testing:**

- **Smoke test**: `optimize(initial, ctx, config, rng)` on a 5-track input completes in well under 1 second and returns a valid permutation (`[0..5].iter().all(|i| result.contains(i))`).
- **Permutation property** (proptest): for any `n ∈ 5..=20` and any seed, `optimize(...)` returns a `Vec<usize>` that is a permutation of `0..n` (sort and compare).
- **Determinism**: two calls to `optimize(initial.clone(), ctx, &config, &mut ChaCha20Rng::seed_from_u64(42))` produce **bit-identical** outputs. Use `pretty_assertions::assert_eq!`.
- **Improvement**: for a synthetic input where the initial ordering is deliberately bad (alphabetical artist clustering), `total_cost(optimize_result) < total_cost(initial)` for at least 9 of 10 random seeds. Use `proptest!` with a moderate case count (`#![proptest_config(ProptestConfig { cases: 10, .. ProptestConfig::default() })]`) or write it as a manual loop.

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --lib algo::anneal`
Expected: all tests pass. Smoke test takes < 1 s wall clock; full suite < 10 s.

Run a performance sanity check (recommended by `implementation-plan-guidance.md`):
```bash
cd /Users/y/Apps/music/order_playlist
cargo test --release --lib algo::anneal::tests::perf_sanity -- --nocapture
```
Expected: 1000 SA iterations on a 40-track playlist completes in under 100 ms.

**Implementation note (no `#[ignore]` permitted — project rule "no silent-skip patterns").** Gate the perf test by `#[cfg(not(debug_assertions))]` so it compiles AND runs only in release builds. Debug builds never see it, so `cargo test` is a no-op for this test in debug; `cargo test --release` runs it. No `#[ignore]`, no skip-with-message.

```rust
#[cfg(not(debug_assertions))]
#[test]
fn perf_sanity_1000_iter_40_track_under_100ms() {
    use std::time::Instant;
    let tracks = synthetic_tracks(40, 1234);
    let ctx = CostContext { tracks: &tracks, weights: CostWeights::default(),
                            arc: EnergyArc, camelot_table: CamelotTable::new() };
    let mut rng = ChaCha20Rng::seed_from_u64(1234);
    let cfg = AnnealConfig { iterations: 1000, restarts: 1, pilot_iterations: 100, ..Default::default() };
    let start = Instant::now();
    let _ = optimize((0..40).collect(), &ctx, &cfg, &mut rng);
    let elapsed = start.elapsed();
    assert!(elapsed.as_millis() < 100, "perf budget exceeded: {:?}", elapsed);
}
```

**Commit:** `Phase 3: simulated annealing with pilot-calibrated T_0 and 2-restart`
<!-- END_TASK_8 -->

<!-- START_TASK_9 -->
### Task 9: Add artist-spacing property test (AC3.5)

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/algo/anneal.rs`

**Implementation:**

The design's AC3.5 says "Property test asserts the invariant over random inputs with default window." This is the algorithmic verification that the heavy `artist_clash` weight actually works.

Test:
```rust
proptest! {
    #![proptest_config(ProptestConfig { cases: 8, .. ProptestConfig::default() })]
    #[test]
    fn artist_spacing_respected_default_window(seed in any::<u64>()) {
        // Build 20 synthetic tracks with 4 distinct artists (5 tracks each).
        // This is intentionally feasible: any well-spread ordering can
        // achieve zero artist clashes.
        let tracks = synthetic_tracks_with_artists(20, 4, seed);
        let ctx = CostContext {
            tracks: &tracks,
            weights: CostWeights::default(), // artist_window=4
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };
        let initial: Vec<usize> = (0..20).collect();
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let result = optimize(initial, &ctx, &AnnealConfig::default(), &mut rng);

        // Assert no two tracks within 4 positions share an artist.
        for i in 0..result.len() {
            for j in (i+1)..(i+5).min(result.len()) {
                let a_i = &tracks[result[i]].query.artist;
                let a_j = &tracks[result[j]].query.artist;
                prop_assert!(!a_i.eq_ignore_ascii_case(a_j),
                    "clash at positions {} and {}: {:?} vs {:?}", i, j, a_i, a_j);
            }
        }
    }
}
```

The synthetic-tracks helper for this test:
```rust
#[cfg(test)]
fn synthetic_tracks_with_artists(n: usize, n_artists: usize, seed: u64) -> Vec<Track> {
    // n tracks distributed round-robin across n_artists distinct artists.
    // Features randomized via a seeded RNG using TrackFeatures::neutral()
    // as a baseline with small per-track perturbations.
}
```

Limit `cases: 8` because each annealing run takes ~100 ms in release mode; 8 cases × 100 ms = 0.8 s per proptest invocation, acceptable.

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo test --release --lib algo::anneal::tests::artist_spacing_respected_default_window`
Expected: passes 8 cases without shrinking failures.

(Run in `--release` because debug-mode SA is too slow for the test.)

**Commit:** `Phase 3: artist-spacing property test (AC3.5)`
<!-- END_TASK_9 -->

<!-- END_SUBCOMPONENT_D -->

<!-- START_SUBCOMPONENT_E (task 10) -->

<!-- START_TASK_10 -->
### Task 10: Re-exports, full verification, commit

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/src/algo/mod.rs`

**Implementation:**

```rust
//! Pure algorithm core — Camelot distance, energy arc, weighted cost,
//! simulated annealing.
//!
//! **FCIS rule:** zero IO, zero async, zero adapter imports.
//! `anyhow::Result` is also banned here — all error surfaces are infallible
//! in this module (the algorithm is pure-by-construction).

pub mod anneal;
pub mod arc;
pub mod camelot;
pub mod cost;

pub use anneal::{optimize, AnnealConfig};
pub use arc::EnergyArc;
pub use camelot::CamelotTable;
pub use cost::{CostContext, CostWeights};
```

Final verification suite:

```bash
cd /Users/y/Apps/music/order_playlist
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --release  # exercises the perf-sanity assertions
cargo build --release
```

Pure-core gate:
```bash
cd /Users/y/Apps/music/order_playlist
grep -rE "use (std::fs|std::io|tokio|reqwest|crate::adapters|crate::cli|anyhow)" src/algo/
```
Expected: zero matches.

If anything fails, fix and rerun until green. **Do not commit a broken state.** Project rule: "If any of these are broken at phase end, the phase is not done."

**Commit:** If Tasks 7 or 9 weren't bundled into a commit already, add a final wrap-up commit:
```bash
git add src/algo/
git status
git commit -m "Phase 3: algo module re-exports + final verification"
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && git status`
Expected: `nothing to commit, working tree clean`.

Run: `cd /Users/y/Apps/music/order_playlist && git log --oneline | head -10`
Expected: Multiple commits since Phase 2 wrap, each starting `Phase 3:`.
<!-- END_TASK_10 -->

<!-- END_SUBCOMPONENT_E -->

---

## Phase 3 Done When

- All property tests pass: delta-cost equivalence, Camelot symmetry, permutation correctness, artist-spacing respect.
- Determinism: `optimize` with `ChaCha20Rng::seed_from_u64(seed)` produces bit-identical output across runs.
- `algo/` imports zero from `adapters/`, `cli/`, `std::fs`, `tokio`, `anyhow`.
- No `anyhow::Result` anywhere in `src/algo/`.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo test --release`, `cargo build --release` all exit 0.
- Doc comments on every `pub` item.
- Coverage on `cost.rs`, `camelot.rs`, `anneal.rs` is ≥ 80% (per project guidance; flagged but not blocking).
- Commits on `playlist-arc-v1` whose subjects start with `Phase 3:`.

## Risk callouts

- **delta_cost off-by-one bugs.** The most likely failure mode of this phase is delta_cost mis-counting which positions are affected by a swap. The property test `delta_cost_equals_total_cost_diff` is the safety net — make sure it actually runs ≥ 256 cases and runs `cargo test --release` to catch subtle drift.
- **Pilot-calibration robustness.** If the pilot run finds zero positive deltas (degenerate input — all tracks identical), guard with a small minimum T₀ (e.g., `T₀ = max(computed, 0.01)`) so the loop doesn't divide by zero or take `ln(0)`.
- **`tracing` events in `algo/`.** Permitted only at function-boundary scope (start/end of `optimize`). Per-iteration `tracing::trace!` would slow the inner loop and is a "side effect in pure module" flag in code review.
- **Performance sanity test.** "1000 SA iterations on a 40-track playlist completes in under 100 ms in release mode" is the project-guidance target. If the perf assertion fails, the most common cause is heap allocations inside the inner loop (reuse a single `Vec<usize>` for the ordering; never `clone()` it per iteration).
