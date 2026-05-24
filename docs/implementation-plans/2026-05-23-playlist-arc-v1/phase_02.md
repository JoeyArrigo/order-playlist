# Playlist Arc v1 — Phase 2: Domain Types

**Goal:** Define newtypes and primitive domain values with validation baked into constructors. No algorithm yet; only data shapes.

**Architecture:** Pure types live in `src/domain/`. No IO, no async, no adapter imports. Constructors reject or clamp invalid input; `pub` items document their invariants.

**Tech Stack:** Rust, serde (derive), tracing (warn on clamp), proptest + test-case (unit tests).

**Scope:** Phase 2 of 7 from `<project-root>/docs/design-plans/2026-05-23-playlist-arc-v1.md`.

**Codebase verified:** 2026-05-23 — Phase 1 scaffolding will exist by the time this phase starts (verified by Phase 1's "Done When"). The `src/domain/mod.rs` stub exists; `newtypes.rs`, `track.rs`, `camelot.rs` do not.

**Project guidance:** `<project-root>/implementation-plan-guidance.md`. Key rules for this phase: doc comments on every `pub` item (or merge is blocked); no `unwrap()` outside tests; pure-core rule enforced (zero `std::fs`/`tokio`/`reqwest`/adapter imports).

---

## Acceptance Criteria Coverage

This phase implements and tests:

**Verifies: None directly.** Phase 2 produces foundation types consumed by later phases. The design plan says "ACs covered: none directly; supports all algorithm ACs." Tests in this phase verify *type invariants*, not user-facing acceptance criteria.

The invariants tested here become preconditions for downstream ACs:
- `Normalized` clamp guarantees feed `playlist-arc-v1.AC2.3` (chart renders without crashing for small N).
- `PitchClass` + `Mode` → `CamelotCode` correctness is the foundation for the Camelot distance term used in `playlist-arc-v1.AC1.1` (output features).

---

## Task Overview

```
SUBCOMPONENT_A: Newtypes with validating constructors (tasks 1-2)
SUBCOMPONENT_B: Track + TrackFeatures types (tasks 3-4)
SUBCOMPONENT_C: Camelot mapping (tasks 5-6)
SUBCOMPONENT_D: Re-exports + commit (task 7)
```

---

<!-- START_SUBCOMPONENT_A (tasks 1-2) -->

<!-- START_TASK_1 -->
### Task 1: Implement Bpm, PitchClass, Mode, Normalized newtypes

**Files:**
- Create: `<project-root>/src/domain/newtypes.rs`

**Implementation:**

Define four newtypes with the following contracts:

- `Bpm(f32)` — beats per minute. Constructor `Bpm::new(value: f32) -> Result<Bpm, DomainError>` rejects non-finite values (`NaN`, infinity), zero, and negatives. Anything else is accepted (we don't impose an upper bound — drum-and-bass legitimately reaches 200+ BPM).
- `PitchClass(u8)` — 0..=11 representing C, C#, D, ... B. Constructor `PitchClass::new(value: u8) -> Result<PitchClass, DomainError>` rejects values ≥ 12.
- `Mode` — `enum { Major, Minor }`. No constructor needed; derive `Copy`, `Eq`, `Hash`. Serde-serialize as lowercase strings (`"major"` / `"minor"`).
- `Normalized(f32)` — value in `[0.0, 1.0]`. Constructor `Normalized::clamp(value: f32) -> Normalized` is **idempotent** (clamp(clamp(x)) == clamp(x)) and emits `tracing::warn!` if the input was outside the range or non-finite (`NaN` → 0.0). A second constructor `Normalized::try_new(value: f32) -> Result<Normalized, DomainError>` is strict (rejects out-of-range and non-finite).

Define a single shared error type for this module:

```rust
#[derive(Debug, thiserror::Error, miette::Diagnostic, PartialEq)]
pub enum DomainError {
    #[error("Bpm must be finite and > 0, got {0}")]
    InvalidBpm(f32),

    #[error("PitchClass must be 0..=11, got {0}")]
    InvalidPitchClass(u8),

    #[error("Normalized value must be finite and in [0.0, 1.0], got {0}")]
    InvalidNormalized(f32),
}
```

Each newtype derives `Debug, Clone, Copy, PartialEq` (no `Eq`/`Hash` on `f32`-wrapping types — that's a clippy lint and a semantic landmine). Each newtype has an accessor `pub fn get(&self) -> f32` (or `u8` for `PitchClass`). No `Default` for `Bpm` or `Normalized` — there's no sensible default. `Mode::default()` returns `Mode::Major` (the algorithm needs a fallback when ReccoBeats omits the field — see Phase 6 risk callout).

Add `#[derive(serde::Serialize, serde::Deserialize)]` on every type so they flow through the cache file (Phase 4) without custom serde impls.

**Module-level doc comment** at the top of the file:
```rust
//! Domain newtypes with validation baked into constructors.
//!
//! `Bpm` and `PitchClass` use fallible `new()` constructors that return
//! `Result<Self, DomainError>`. `Normalized` uses a forgiving `clamp()`
//! constructor that emits a `tracing::warn!` on out-of-range input and a
//! strict `try_new()` for callers who would rather see the error.
```

Doc comment on every `pub` item (block-comment style for types, `///` for methods). Project rule: "New `pub` items without doc comments block the merge."

**Pre-validated fallback constants.** Adapters (Phase 6) sometimes need to construct a newtype from a known-safe constant where calling `new()` and unwrapping the `Result` is wasteful. Add these `pub const` definitions inside the same module so adapters can write `Bpm::DEFAULT_120` instead of `Bpm::new(120.0).expect("120 is valid")`:

```rust
impl Bpm {
    /// The default fallback BPM (120). Pre-validated — bypasses `new()`.
    pub const DEFAULT_120: Bpm = Bpm(120.0);
}

impl PitchClass {
    /// C (pitch class 0). Pre-validated — bypasses `new()`.
    pub const C: PitchClass = PitchClass(0);
}

impl Normalized {
    /// 0.0 — pre-validated, bypasses `clamp()`.
    pub const ZERO: Normalized = Normalized(0.0);
    /// 0.5 — pre-validated, bypasses `clamp()`.
    pub const HALF: Normalized = Normalized(0.5);
    /// 1.0 — pre-validated, bypasses `clamp()`.
    pub const ONE: Normalized = Normalized(1.0);
}
```

Why constants instead of `unwrap_or_else(|| X::new(c).expect("..."))`: project rule "no `unwrap()` outside tests/`main`" — `expect()` is technically allowed with justification, but a `const` is unambiguously safe (the compiler verifies the value).

**Testing:**

Tests must verify these invariants (place under `#[cfg(test)] mod tests` in the same file):

- `Bpm::new` rejects: `NaN`, `+inf`, `-inf`, `0.0`, `-1.0`.
- `Bpm::new` accepts: `60.0`, `120.5`, `200.0`.
- `PitchClass::new` rejects: `12`, `100`, `255`.
- `PitchClass::new` accepts: all of `0..=11` (parameterize with `test_case` so each is its own test case).
- `Normalized::try_new` rejects `NaN`, `-0.001`, `1.001`, `+inf`.
- `Normalized::clamp` idempotence — **property test** via `proptest`: for any `f32`, `Normalized::clamp(x).get() == Normalized::clamp(Normalized::clamp(x).get()).get()`.
- `Normalized::clamp(NaN).get() == 0.0` (explicit unit test — proptest's `f32` generator includes NaN but the equality assertion needs special care).
- `Normalized::clamp(-1.5).get() == 0.0`; `Normalized::clamp(2.0).get() == 1.0`.

Task-implementor: use `assert_eq!` from `pretty_assertions` for nicer diffs. Use `test_case::test_case` macro for parameterized cases.

**Verification:**

Run: `cd <project-root> && cargo test --lib domain::newtypes -- --nocapture`
Expected: All cases pass. Property test should run ≥ 256 cases (proptest default) without shrinking failures.

Run: `cd <project-root> && cargo clippy --all-targets -- -D warnings`
Expected: Clean.

**Commit:** `Phase 2: domain newtypes (Bpm, PitchClass, Mode, Normalized)`
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Wire newtypes into domain/mod.rs and verify pure-core invariant

**Files:**
- Modify: `<project-root>/src/domain/mod.rs`

**Implementation:**

Add `pub mod newtypes;` to `domain/mod.rs`. Add a re-export so callers can `use crate::domain::{Bpm, PitchClass, Mode, Normalized}` without typing `newtypes::`:

```rust
pub mod newtypes;

pub use newtypes::{Bpm, DomainError, Mode, Normalized, PitchClass};
```

**Pure-core enforcement.** After this task, run:

```bash
cd <project-root>
grep -rE "use (std::fs|tokio|reqwest|crate::adapters|crate::cli)" src/domain/
```

Expected: zero matches. If anything matches, fix the import — the FCIS rule is the load-bearing pattern of this project (per `implementation-plan-guidance.md` and the design's "Existing Patterns" section).

**Verification:**

Run: `cd <project-root> && cargo build && cargo test --lib`
Expected: Build green; all tests still pass.

**Commit:** Bundled with Task 1's commit (squash, or use `git commit --amend` if Task 1 was already committed — both files belong to the "newtypes" deliverable).
<!-- END_TASK_2 -->

<!-- END_SUBCOMPONENT_A -->

<!-- START_SUBCOMPONENT_B (tasks 3-4) -->

<!-- START_TASK_3 -->
### Task 3: Implement TrackQuery, TrackId, Track, TrackFeatures

**Files:**
- Create: `<project-root>/src/domain/track.rs`

**Implementation:**

Define the four types the optimizer and adapters pass around. The annealer (Phase 3) only ever sees fully-resolved `Track` values — it never has to handle `Option<TrackFeatures>` (the adapter boundary collapses partial-data handling, per design "Architecture" section).

- `TrackQuery { title: String, artist: String }` — input from CSV. Derives `Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize`. The `Hash`+`Eq` are required because Phase 4's cache uses `HashMap<TrackQuery, TrackId>`. Provide a constructor `TrackQuery::new(title: impl Into<String>, artist: impl Into<String>) -> Self` that trims whitespace on both fields (`title.trim().to_string()`).
- `TrackId(pub(crate) String)` — opaque identifier (ISRC in v1; later providers may use Spotify IDs or UUIDs). The inner field is `pub(crate)` not `pub` so `algo/` cannot pattern-match on it. Provide `TrackId::new(s: impl Into<String>) -> Self` (no validation — any non-empty string is accepted; emit `tracing::warn!` if empty). Derives `Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize`.
- `Track { query: TrackQuery, id: TrackId, features: TrackFeatures }` — fully-resolved track. Derives `Debug, Clone, serde::Serialize, serde::Deserialize`.
- `TrackFeatures` — see Task 4.

Module-level doc comment explains the FCIS contract: "These types are the only thing the algorithm sees. Adapters collapse all `Option`/`Result` partial-data handling before constructing `Track`."

**Testing:**

- `TrackQuery::new("  Hello  ", "World")` produces `TrackQuery { title: "Hello", artist: "World" }`.
- `TrackQuery` equality is case-sensitive on both fields (compare `("Daft Punk", "Get Lucky")` vs `("daft punk", "Get Lucky")` — must NOT be equal). The case-insensitive *artist match* used by the artist-window constraint is enforced in Phase 3, not here.
- `TrackId::new("")` produces a TrackId whose `get()` returns `""` and emits a warn-level tracing event (use `tracing_test` or a custom subscriber if you want to assert the event; otherwise leave the warning untested — the project doesn't require coverage on tracing events).

**Verification:**

Run: `cd <project-root> && cargo test --lib domain::track`
Expected: All tests pass.

**Commit:** `Phase 2: TrackQuery, TrackId, Track types`
<!-- END_TASK_3 -->

<!-- START_TASK_4 -->
### Task 4: Implement TrackFeatures struct

**Files:**
- Modify: `<project-root>/src/domain/track.rs`

**Implementation:**

`TrackFeatures` holds the numeric values produced by the feature-source adapter (ReccoBeats in Phase 6). Field list comes from design `playlist-arc-v1.AC1.1`: `tempo, key, mode, energy, danceability, valence, loudness`. The design says "all ReccoBeats fields except `time_signature`" — keep the four extra fields ReccoBeats also exposes (`acousticness`, `instrumentalness`, `liveness`, `speechiness`) for completeness, but only the seven AC1.1 fields are part of the output CSV.

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackFeatures {
    pub tempo: Bpm,
    pub key: PitchClass,
    pub mode: Mode,
    pub energy: Normalized,
    pub danceability: Normalized,
    pub valence: Normalized,
    /// dB; ReccoBeats returns values typically in [-60.0, 0.0] but no
    /// clamp is enforced — extreme values are passed through.
    pub loudness: f32,
    pub acousticness: Normalized,
    pub instrumentalness: Normalized,
    pub liveness: Normalized,
    pub speechiness: Normalized,
}
```

Provide one helper:
```rust
impl TrackFeatures {
    /// A neutral mid-curve placeholder used by tests and by Phase 7's
    /// snapshot fixtures. NOT used in production — adapters always
    /// populate real values.
    #[cfg(test)]
    pub fn neutral() -> Self { /* tempo=120, key=0, mode=Major, all Normalized=0.5, loudness=-10.0 */ }
}
```

The `neutral()` helper is `#[cfg(test)]`-only so it never ships in release builds.

**Risk callout (will be repeated in Phase 6 plan):** ReccoBeats research suggests `key` and `mode` may NOT be returned by `/v1/audio-features` in 2026. If Phase 6 implementation discovers this, the ReccoBeats adapter will fall back to `PitchClass::new(0).unwrap()` and `Mode::Major` for both — and the algorithm's Camelot term will degrade to "all tracks treated as same key." That degradation is acceptable for v1 (the energy-arc term dominates the cost function), but the executor must explicitly decide and document the fallback in Phase 6 if the API gap is real.

**Testing:**

- `TrackFeatures::neutral()` round-trips through `serde_json::to_string` and `serde_json::from_str` without loss (use `pretty_assertions::assert_eq!`).
- `Track { query, id, features: TrackFeatures::neutral() }` serializes to JSON and deserializes back to an equal value (`Track` needs `PartialEq` for this — derive it; `TrackFeatures` needs it too — derive `PartialEq` on it as well, using `f32` field equality which is fine for round-trip tests but should be commented on).

If `PartialEq` on `TrackFeatures` proves problematic (NaN, sign-of-zero), wrap each `f32` field's equality in a tolerance helper inside the test. Don't derive `Eq`/`Hash` — never on `f32` newtypes.

**Verification:**

Run: `cd <project-root> && cargo test --lib domain::track`
Expected: All tests pass, including the serde round-trip.

Run: `cd <project-root> && cargo clippy --all-targets -- -D warnings`
Expected: Clean. (Watch for `clippy::float_cmp` on the round-trip equality test — silence with `#[allow(clippy::float_cmp)]` on the test function if necessary, and add a one-line comment explaining why bitwise equality is acceptable for serde round-trip.)

**Commit:** `Phase 2: TrackFeatures with serde round-trip test`
<!-- END_TASK_4 -->

<!-- END_SUBCOMPONENT_B -->

<!-- START_SUBCOMPONENT_C (tasks 5-6) -->

<!-- START_TASK_5 -->
### Task 5: Implement Camelot mapping (CamelotLetter, CamelotCode)

**Files:**
- Create: `<project-root>/src/domain/camelot.rs`

**Implementation:**

The Camelot wheel maps musical key + mode to a 24-position notation used by DJs for harmonic mixing. Adjacent positions on the wheel are harmonically compatible.

Standard mapping (pitch class → Camelot number):

| Pitch class | Note | Major (B side) | Minor (A side) |
|-------------|------|----------------|----------------|
| 0  | C  | 8B  | 5A  |
| 1  | C#/Db | 3B | 12A |
| 2  | D  | 10B | 7A  |
| 3  | D#/Eb | 5B | 2A  |
| 4  | E  | 12B | 9A  |
| 5  | F  | 7B  | 4A  |
| 6  | F#/Gb | 2B | 11A |
| 7  | G  | 9B  | 6A  |
| 8  | G#/Ab | 4B | 1A  |
| 9  | A  | 11B | 8A  |
| 10 | A#/Bb | 6B | 3A  |
| 11 | B  | 1B  | 10A |

This is the conventional 12-tone-equal-temperament Camelot mapping used by Mixed In Key, Beatport, Rekordbox, etc.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CamelotLetter { A, B }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CamelotCode {
    pub number: u8,  // 1..=12
    pub letter: CamelotLetter,
}

impl From<(PitchClass, Mode)> for CamelotCode {
    fn from((pc, mode): (PitchClass, Mode)) -> Self {
        // Table lookup based on the standard chart above.
        // Mode::Major → CamelotLetter::B, Mode::Minor → CamelotLetter::A.
    }
}

impl CamelotCode {
    /// Returns a 0..=23 index for use as a row/column in the
    /// CamelotTable distance lookup (Phase 3).
    ///
    /// **Convention (load-bearing — used by Phase 3's CamelotTable):**
    /// - B ring (major): index = number - 1 → 0..=11
    ///   (1B → 0, 2B → 1, ..., 12B → 11)
    /// - A ring (minor): index = 11 + number → 12..=23
    ///   (1A → 12, 2A → 13, ..., 12A → 23)
    pub fn index(&self) -> u8 {
        match self.letter {
            CamelotLetter::B => self.number - 1,
            CamelotLetter::A => 11 + self.number,
        }
    }
}
```

**Testing:**

- 24 parameterized cases using `test_case::test_case`: every `(PitchClass, Mode)` combination produces the expected `CamelotCode`. Include all 12 major and all 12 minor explicitly.
- `CamelotCode::index()` returns a unique value in `0..=23` for every Camelot code (property test using `proptest::collection::vec` or just an exhaustive sweep of the 24 inputs).
- Round-trip: every `(PitchClass, Mode)` → `CamelotCode` → `index()` → reverse-lookup produces the original `(PitchClass, Mode)`. The reverse-lookup function is not required as `pub` — keep it `pub(crate)` or inside the test module.

**Verification:**

Run: `cd <project-root> && cargo test --lib domain::camelot`
Expected: 24 parameterized cases pass; bijection property holds.

**Commit:** `Phase 2: Camelot mapping (PitchClass, Mode) -> CamelotCode`
<!-- END_TASK_5 -->

<!-- START_TASK_6 -->
### Task 6: Wire camelot + track modules into domain/mod.rs

**Files:**
- Modify: `<project-root>/src/domain/mod.rs`

**Implementation:**

Add the new modules and update re-exports so consumers can `use crate::domain::*` and get every type:

```rust
//! Pure domain types — newtypes, identifiers, value objects.
//!
//! **FCIS rule:** This module MUST NOT import `std::fs`, `std::io`, `tokio`,
//! `reqwest`, or anything from `crate::adapters` or `crate::cli`.

pub mod camelot;
pub mod newtypes;
pub mod track;

pub use camelot::{CamelotCode, CamelotLetter};
pub use newtypes::{Bpm, DomainError, Mode, Normalized, PitchClass};
pub use track::{Track, TrackFeatures, TrackId, TrackQuery};
```

**Verification:**

Run: `cd <project-root> && cargo build && cargo test --lib domain`
Expected: Build green, all domain tests pass.

Re-check the pure-core rule:
```bash
cd <project-root>
grep -rE "use (std::fs|std::io|tokio|reqwest|crate::adapters|crate::cli)" src/domain/
```
Expected: zero matches.

**No new commit yet** — bundle with Task 7.
<!-- END_TASK_6 -->

<!-- END_SUBCOMPONENT_C -->

<!-- START_SUBCOMPONENT_D (task 7) -->

<!-- START_TASK_7 -->
### Task 7: Run full verification suite and commit

**Files:** None modified.

**Implementation:**

Run the four mandatory verification commands from `implementation-plan-guidance.md`:

```bash
cd <project-root>
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

All four MUST exit 0. `cargo test` should report at least the following test counts:
- `domain::newtypes::tests` — at least 8 unit tests + 1 proptest.
- `domain::track::tests` — at least 3 unit tests.
- `domain::camelot::tests` — at least 25 (24 parameterized + bijection).

If any prior task wasn't committed (e.g., Task 6 was bundled with this task), stage and commit now:

```bash
cd <project-root>
git add src/domain/
git status
git commit -m "Phase 2: wire domain module re-exports"
```

**Verification:**

Run: `cd <project-root> && git log --oneline | head -5`
Expected: 3–4 new commits since Phase 1's commit, each subject starting with `Phase 2:`. (Squashing Tasks 1–2 and 5–6 into single commits is fine — what matters is each logical deliverable is a commit, not the per-task count.)

Run: `cd <project-root> && git status`
Expected: `nothing to commit, working tree clean`.
<!-- END_TASK_7 -->

<!-- END_SUBCOMPONENT_D -->

---

## Phase 2 Done When

- All newtypes are `pub` with validating or clamping constructors.
- No `Track` field uses a bare primitive where a newtype exists (`tempo: Bpm`, not `tempo: f32`).
- `domain::newtypes`, `domain::track`, `domain::camelot` all exist with passing unit + property tests.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --release` all exit 0.
- `grep -rE "use (std::fs|std::io|tokio|reqwest|crate::adapters|crate::cli)" src/domain/` returns zero matches.
- All `pub` items have doc comments.
- Commits on `playlist-arc-v1` whose subjects start with `Phase 2:`.

## Risk callouts

- **`f32` equality.** `TrackFeatures` derives `PartialEq` for round-trip tests; this is fine for the constant `neutral()` fixture but is a foot-gun for floats in general. Comment it inline.
- **ReccoBeats key/mode gap (forward warning).** If Phase 6 finds `/v1/audio-features` doesn't return key/mode, `TrackFeatures::key` and `TrackFeatures::mode` will be defaulted at the adapter boundary, weakening the Camelot cost term. The newtype contract here is unchanged — it just becomes less informative in practice.
