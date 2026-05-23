//! Pure algorithm core — cost function, energy arc, Camelot distance,
//! simulated-annealing loop.
//!
//! **FCIS rule:** Identical to `crate::domain` — zero IO, zero async,
//! zero adapter imports. The annealer takes borrows of fully-validated
//! `Track` values and a seeded `rand::Rng`; it never sees `Option<Features>`.

// Submodules added in Phase 3: camelot, arc, cost, anneal.
