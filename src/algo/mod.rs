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
#[cfg(test)]
pub mod test_support;

pub use anneal::{optimize, AnnealConfig};
pub use arc::EnergyArc;
pub use camelot::CamelotTable;
pub use cost::{CostContext, CostWeights};
