//! Pure algorithm core — Camelot distance, energy arc, weighted cost,
//! simulated annealing.
//!
//! **FCIS rule:** zero IO, zero async, zero adapter imports.
//! `anyhow::Result` is also banned here — all error surfaces are infallible
//! in this module (the algorithm is pure-by-construction).

pub mod camelot;

pub use camelot::CamelotTable;
