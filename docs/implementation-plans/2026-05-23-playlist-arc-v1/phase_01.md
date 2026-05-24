# Playlist Arc v1 — Phase 1: Project Scaffolding

**Goal:** Initialize the Rust crate with pinned dependencies, Cargo features, and empty module skeletons. Produce a clean `cargo build` + `cargo test` + `cargo clippy` baseline.

**Architecture:** Single binary crate with a functional core / imperative shell layout. `domain/` and `algo/` are pure modules; `adapters/` and `cli/` hold IO. Cargo features select adapter implementations at compile time.

**Tech Stack:** Rust (edition 2021), Cargo, tokio, reqwest (rustls-tls), serde, clap, miette, thiserror, tracing.

**Scope:** Phase 1 of 7 from `/Users/y/Apps/music/order_playlist/docs/design-plans/2026-05-23-playlist-arc-v1.md`.

**Codebase verified:** 2026-05-23 — greenfield. No `Cargo.toml`, no `src/`, no `tests/`. Branch `playlist-arc-v1` checked out. `cargo 1.61.0` / `rustc 1.61.0` present (may need rustup upgrade — note in Task 1 verification).

**Project guidance:** `/Users/y/Apps/music/order_playlist/implementation-plan-guidance.md`. Key rules: no `unwrap()` outside tests/`main`; missing doc comments on `pub` items block merge; cargo fmt + clippy + test + build must all pass at end of every phase.

---

## Acceptance Criteria Coverage

This phase implements and tests:

**Verifies: None** (infrastructure phase — no behavior to test).

The phase's success is verified operationally by build/test/clippy commands. It establishes the foundation that supports all later ACs, in particular:
- `playlist-arc-v1.AC8.1` — `cargo build --no-default-features --features musicbrainz,reccobeats` succeeds (verified by Task 5).
- `playlist-arc-v1.AC9.1` — Default `cargo test` issues zero network requests (verified here by `cargo test` producing zero test runs — full enforcement comes once tests exist).

---

## Task Overview

```
SUBCOMPONENT_A: Cargo manifest + ignore files (tasks 1-2)
SUBCOMPONENT_B: Source skeleton (tasks 3-5)
SUBCOMPONENT_C: Build matrix verification (task 6)
SUBCOMPONENT_D: Commit (task 7)
```

---

<!-- START_SUBCOMPONENT_A (tasks 1-2) -->

<!-- START_TASK_1 -->
### Task 1: Initialize Cargo manifest with pinned dependencies and feature flags

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/Cargo.toml`

**Implementation:**

Create a binary-crate `Cargo.toml`. Pin every dependency exactly (`=x.y.z`) per the project rule "Dependencies pinned exactly — application binary, not a published library." Define four Cargo features: `default = ["musicbrainz", "reccobeats"]`, `musicbrainz`, `reccobeats`, `live-network`. The `live-network` feature MUST NOT alter behavior of the default test suite — it only gates a few opt-in HTTP tests created in Phases 5/6.

Use **edition = "2021"** (current stable). Use `rustls-tls` rather than `native-tls` for reqwest so the build works on macOS without OpenSSL.

```toml
[package]
name = "order_playlist"
version = "0.1.0"
edition = "2021"
description = "Reorder a CSV playlist to follow a target energy arc."
publish = false

[[bin]]
name = "order_playlist"
path = "src/main.rs"

[features]
default = ["musicbrainz", "reccobeats"]
musicbrainz = []
reccobeats = []
live-network = []

[dependencies]
clap = { version = "=4.6.1", features = ["derive"] }
tokio = { version = "=1.52.3", features = ["full"] }
reqwest = { version = "=0.13.3", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "=1.0.228", features = ["derive"] }
serde_json = "=1.0.149"
csv = "=1.3.3"
rand = "=0.10.0"
rand_chacha = "=0.10.0"
thiserror = "=2.0.18"
miette = { version = "=7.6.0", features = ["fancy"] }
tracing = "=0.1.44"
tracing-subscriber = { version = "=0.3.20", features = ["env-filter", "fmt"] }
anyhow = "=1.0.98"

[dev-dependencies]
proptest = "=1.9.0"
insta = "=1.47.2"
test-case = "=3.3.1"
pretty_assertions = "=1.4.1"

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo --version`
Expected: A version ≥ 1.75 (edition 2021 features used in later phases). If the local toolchain reports < 1.75, run `rustup update stable` before continuing. If `rustup` is not installed, install it from https://rustup.rs and re-run.

Run: `cd /Users/y/Apps/music/order_playlist && cargo metadata --offline 2>&1 | head -5`
Expected: Errors gracefully OR shows metadata. Either is acceptable — this just confirms `Cargo.toml` parses.

**MANDATORY pin verification before `cargo build`.** The `rand = "=0.10.0"` and `rand_chacha = "=0.10.0"` versions in the manifest above were derived from speculative research; they may not exist on crates.io. Before `cargo build`, run:

```bash
cd /Users/y/Apps/music/order_playlist
cargo search rand --limit 1
cargo search rand_chacha --limit 1
```

Each prints the latest version available. If `=0.10.0` doesn't match, edit `Cargo.toml` and pin the actual latest stable version (e.g., `rand = "=0.9.2"`, `rand_chacha = "=0.9.0"`). Document the substitution in the commit message at Task 7. Repeat the same verification for any other dep that fails to resolve on first `cargo build`.

**No commit yet** — commits happen at Task 7 after all scaffolding files exist.
<!-- END_TASK_1 -->

<!-- START_TASK_2 -->
### Task 2: Update .gitignore and add .env.example

**Files:**
- Modify: `/Users/y/Apps/music/order_playlist/.gitignore` (existing — 4 lines)
- Create: `/Users/y/Apps/music/order_playlist/.env.example`

**Implementation:**

The existing `.gitignore` contains `/target`, `*.cache.json`, `.env`, `.DS_Store`. Add Rust-specific patterns and IDE noise:

Append to `.gitignore`:
```
# Rust
/target/
**/*.rs.bk
*.pdb

# insta snapshot review pending
*.snap.new

# IDE
.vscode/
.idea/
*.iml
```

(The existing `/target` line stays; the regex `/target/` here is redundant but safe.)

Create `.env.example`:
```
# Placeholder for future credentials.
# v1 does not require any environment variables — MusicBrainz and
# ReccoBeats are no-auth in this design.
#
# Copy this file to `.env` and fill in values when later providers
# (e.g., Spotify, authenticated MusicBrainz) are added.

# Example placeholders (commented):
# SPOTIFY_CLIENT_ID=
# SPOTIFY_CLIENT_SECRET=
# MUSICBRAINZ_USER_AGENT_CONTACT=your-email@example.com
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cat .gitignore | grep -E 'target|cache\.json|\.env|DS_Store|snap\.new'`
Expected: All five patterns present.

Run: `cd /Users/y/Apps/music/order_playlist && test -f .env.example && echo OK`
Expected: `OK`.

**No commit yet.**
<!-- END_TASK_2 -->

<!-- END_SUBCOMPONENT_A -->

<!-- START_SUBCOMPONENT_B (tasks 3-5) -->

<!-- START_TASK_3 -->
### Task 3: Create src/main.rs, src/lib.rs, and errors.rs placeholders

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/src/main.rs`
- Create: `/Users/y/Apps/music/order_playlist/src/lib.rs`
- Create: `/Users/y/Apps/music/order_playlist/src/errors.rs`

**Implementation:**

These are entry points and a stub for the structured error types. They must compile with **zero warnings** under `cargo clippy --all-targets -- -D warnings`. Real content lands in later phases — the stubs only have to satisfy the linker and clippy.

`src/main.rs`:
```rust
//! `order_playlist` binary entry point.
//!
//! Full orchestration lands in Phase 7. This stub exits 0 so the build
//! matrix is green from Phase 1 onward.

fn main() {
    // Phase 7 will replace this with miette panic-hook install, clap parse,
    // tracing-subscriber init, and the pipeline orchestration.
}
```

`src/lib.rs`:
```rust
//! `order_playlist` library crate.
//!
//! Modules are added as later phases fill them in. Re-exports here let
//! integration tests under `tests/` reach internal types without bypassing
//! visibility rules.

pub mod adapters;
pub mod algo;
pub mod cli;
pub mod domain;
pub mod errors;
```

`src/errors.rs`:
```rust
//! Structured error types. Concrete variants land in later phases:
//! `InputError` (Phase 4), `CacheError` (Phase 4), `AdapterError` (Phases 5–6).
//!
//! Each error type uses `thiserror::Error` and derives
//! `miette::Diagnostic` for user-facing source spans / help text.
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo check 2>&1 | tail -20`
Expected: Compiles cleanly OR errors only about missing `adapters/algo/cli/domain` modules (those are created in Task 4). No syntax errors in the three files above.

**No commit yet.**
<!-- END_TASK_3 -->

<!-- START_TASK_4 -->
### Task 4: Create empty module skeletons for domain, algo, adapters, cli

**Files:**
- Create: `/Users/y/Apps/music/order_playlist/src/domain/mod.rs`
- Create: `/Users/y/Apps/music/order_playlist/src/algo/mod.rs`
- Create: `/Users/y/Apps/music/order_playlist/src/adapters/mod.rs`
- Create: `/Users/y/Apps/music/order_playlist/src/cli/mod.rs`

**Implementation:**

Each `mod.rs` is a re-export file only. Real submodules land in later phases. Each file must include a top-level doc comment naming the FCIS role of the module — these double as enforced documentation for the pure-core / impure-shell split.

`src/domain/mod.rs`:
```rust
//! Pure domain types — newtypes, identifiers, value objects.
//!
//! **FCIS rule:** This module MUST NOT import `std::fs`, `std::io`, `tokio`,
//! `reqwest`, or anything from `crate::adapters` or `crate::cli`. Any code
//! review that introduces such an import blocks the merge.

// Submodules added in Phase 2: newtypes, track, camelot.
```

`src/algo/mod.rs`:
```rust
//! Pure algorithm core — cost function, energy arc, Camelot distance,
//! simulated-annealing loop.
//!
//! **FCIS rule:** Identical to `crate::domain` — zero IO, zero async,
//! zero adapter imports. The annealer takes borrows of fully-validated
//! `Track` values and a seeded `rand::Rng`; it never sees `Option<Features>`.

// Submodules added in Phase 3: camelot, arc, cost, anneal.
```

`src/adapters/mod.rs`:
```rust
//! Impure adapter shell — file IO, HTTP, JSON cache.
//!
//! Adapter trait definitions and feature-gated implementations live here.
//! The default v1 implementations (`musicbrainz`, `reccobeats`) are gated
//! by Cargo features of the same name. Adding a new provider means adding
//! a new feature flag and a new impl; nothing in `algo/` or `domain/`
//! should change.

// Submodules added in later phases: csv_io, cache (Phase 4),
// musicbrainz (Phase 5), reccobeats (Phase 6).
```

`src/cli/mod.rs`:
```rust
//! CLI presentation — clap argument structs, ASCII chart, summary
//! report formatter.
//!
//! This module owns terminal output. The `main.rs` binary is the only
//! other place permitted to call `println!`/`eprintln!` directly.

// Submodules added in Phase 7: args, chart, report.
```

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && cargo build 2>&1 | tail -10`
Expected: `Compiling order_playlist v0.1.0 ...` followed by `Finished ...`. Zero errors, zero warnings.

Run: `cd /Users/y/Apps/music/order_playlist && cargo test 2>&1 | tail -10`
Expected: Compiles and runs `0 passed; 0 failed`.

**No commit yet.**
<!-- END_TASK_4 -->

<!-- START_TASK_5 -->
### Task 5: Verify Cargo-feature matrix builds clean

**Files:** None modified. Verification only.

**Implementation:**

The design has four feature flags. Validate each combination required by the DoD:

1. Default build (`musicbrainz` + `reccobeats` enabled): `cargo build`
2. Explicit features only (matches AC8.1): `cargo build --no-default-features --features musicbrainz,reccobeats`
3. Live-network feature compiles: `cargo build --features live-network`
4. Bare-bones (AC8.3 — pure algo+domain only): `cargo build --no-default-features`

In Phase 1 every feature combination is behaviorally identical because no feature-gated code exists yet. The test here is that the manifest is syntactically correct and the feature flags don't introduce dependency conflicts.

**Verification:**

Run each, in order. Each must finish with `Finished ...` and zero warnings.

```bash
cd /Users/y/Apps/music/order_playlist
cargo build
cargo build --no-default-features --features musicbrainz,reccobeats
cargo build --features live-network
cargo build --no-default-features
```

Then:

```bash
cd /Users/y/Apps/music/order_playlist
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Expected: Clippy clean, formatter happy.

If `cargo fmt --check` reports diffs, run `cargo fmt` and re-verify.

**No commit yet.**
<!-- END_TASK_5 -->

<!-- END_SUBCOMPONENT_B -->

<!-- START_SUBCOMPONENT_C (task 6) -->

<!-- START_TASK_6 -->
### Task 6: Run the full project-guidance verification suite

**Files:** None modified.

**Implementation:**

Run the verification commands listed in `/Users/y/Apps/music/order_playlist/implementation-plan-guidance.md` ("What 'done' looks like per phase"):

```bash
cd /Users/y/Apps/music/order_playlist
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

**Verification:**

All four commands MUST exit 0. `cargo test` must report `0 passed; 0 failed; 0 ignored`. There must be no `#[ignore]` annotations anywhere — the project rule is "no silent-skip patterns" (project-guidance.testing).

If `cargo build --release` is slow on first run (release profile uses `lto = "thin"`), let it finish — the release profile is exercised by the CI matrix and the eventual `cargo run --release`.
<!-- END_TASK_6 -->

<!-- END_SUBCOMPONENT_C -->

<!-- START_SUBCOMPONENT_D (task 7) -->

<!-- START_TASK_7 -->
### Task 7: Commit Phase 1 scaffolding

**Files:** All new files from Tasks 1–4 plus the modified `.gitignore`.

**Implementation:**

Stage the new files explicitly — do NOT use `git add .` (project rule per Claude Code defaults: avoid accidentally staging `.env` or other untracked secrets).

```bash
cd /Users/y/Apps/music/order_playlist

git add Cargo.toml
git add .gitignore
git add .env.example
git add src/main.rs src/lib.rs src/errors.rs
git add src/domain/mod.rs src/algo/mod.rs src/adapters/mod.rs src/cli/mod.rs

# Cargo.lock is created by `cargo build` — commit it for a binary crate
# (project rule: pinned deps, reproducible builds).
git add Cargo.lock

git status
```

Verify `git status` shows only the intended files staged. Then commit:

```bash
git commit -m "Phase 1: project scaffolding (Cargo.toml, src skeleton, feature matrix)"
```

If the executor is configured to add a Co-Authored-By trailer, include it; otherwise the bare subject line is fine.

**Verification:**

Run: `cd /Users/y/Apps/music/order_playlist && git log --oneline -1`
Expected: A new commit on `playlist-arc-v1` whose subject begins `Phase 1:`.

Run: `cd /Users/y/Apps/music/order_playlist && git status`
Expected: `nothing to commit, working tree clean`.

Run one more full verification pass to confirm the committed state is green:
```bash
cd /Users/y/Apps/music/order_playlist
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```
All three exit 0.
<!-- END_TASK_7 -->

<!-- END_SUBCOMPONENT_D -->

---

## Phase 1 Done When

- `cargo build` succeeds.
- `cargo build --no-default-features --features musicbrainz,reccobeats` succeeds.
- `cargo build --features live-network` succeeds.
- `cargo build --no-default-features` succeeds.
- `cargo test` runs zero tests and exits 0.
- `cargo clippy --all-targets -- -D warnings` is clean.
- `cargo fmt --check` reports no diffs.
- One commit on branch `playlist-arc-v1` whose subject starts with `Phase 1:`.

## Risk callouts

- **Toolchain version.** Investigation found local `cargo 1.61.0` (2022). The pinned deps assume modern stable. If `cargo build` fails with edition or MSRV errors, run `rustup update stable` before debugging the manifest.
- **rand 0.10 / rand_chacha 0.10.** Pinned versions are based on May-2026 research; if they don't resolve, fall back to the latest 0.x available on crates.io and update the commit message.
