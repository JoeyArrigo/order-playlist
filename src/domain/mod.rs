//! Pure domain types — newtypes, identifiers, value objects.
//!
//! **FCIS rule:** This module MUST NOT import `std::fs`, `std::io`, `tokio`,
//! `reqwest`, or anything from `crate::adapters` or `crate::cli`. Any code
//! review that introduces such an import blocks the merge.

pub mod camelot;
pub mod newtypes;
pub mod track;

pub use camelot::{CamelotCode, CamelotLetter};
pub use newtypes::{Bpm, DomainError, Mode, Normalized, PitchClass};
pub use track::{Track, TrackFeatures, TrackId, TrackQuery};
