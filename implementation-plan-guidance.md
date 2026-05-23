# Implementation plan guidance for party-playlist

Loaded by `/start-implementation-plan` and used during final code review.
Defines coding standards, testing requirements, and review criteria specific
to this project. Style and language defaults are in
`design-plan-guidance.md` — this file covers verification and review.

## Testing requirements

- **Pure modules require unit tests.** `cost.rs`, `camelot.rs`, `anneal.rs`
  must have ≥80% line coverage. The cost function and Camelot distance
  table are pure functions of inputs — there is no excuse not to test them.
- **Delta-cost must be tested against full-recompute.** Property test:
  for any 2-swap on a random ordering, `cost(after) - cost(before)` must
  equal `delta_cost(before, swap)`. This is the single most important
  correctness invariant in the project.
- **Determinism test required.** Run the annealer twice with the same
  seed and same inputs; assert identical output orderings.
- **Artist-spacing constraint enforced post-hoc.** A test must scan the
  output of a real annealing run and assert no two same-artist tracks
  appear within N positions.
- **Spotify adapter has mocked tests only.** Don't hit the live API in
  CI. Record fixtures of real responses in `tests/fixtures/` and replay.
- **Integration test:** a tiny 10-song CSV (committed to the repo) runs
  end-to-end through the cached path (no network) and produces a stable,
  deterministic ordering.

## Code review criteria (block on these)

- Any `unwrap()` outside tests/`main` blocks the merge.
- Any annealing iteration that does a full cost recompute blocks the merge.
- Missing unit tests on `cost.rs`, `camelot.rs`, or `anneal.rs` block the merge.
- Side effects (I/O, randomness) in pure modules block the merge.
- New `pub` items without doc comments block the merge.
- Errors swallowed with `let _ = ...` block the merge unless commented.

## Code review criteria (flag, don't block)

- Functions >50 lines.
- Files >300 lines (consider splitting).
- Allocations inside the annealing hot loop (consider reusing buffers).
- Unbounded retries on Spotify calls (should respect 429 and have a max).

## Verification commands

Each phase's tasks should specify verification. Use these by default:

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Performance-sensitive phases (annealing, delta-cost) should also include
a microbenchmark assertion — e.g. "1000 SA iterations on a 40-track
playlist completes in under 100ms in release mode." Use `criterion` or a
hand-rolled timer test gated behind `--release`.

## What "done" looks like per phase

After each phase, the codebase should be:
- Compiling clean (`cargo build --release` succeeds).
- Lint-clean (`cargo clippy -- -D warnings`).
- Test-passing (`cargo test`).
- Committed with a message that names the phase.

If any of these are broken at phase end, the phase is not done.
