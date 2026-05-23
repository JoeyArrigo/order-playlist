//! Pure domain types — newtypes, identifiers, value objects.
//!
//! **FCIS rule:** This module MUST NOT import `std::fs`, `std::io`, `tokio`,
//! `reqwest`, or anything from `crate::adapters` or `crate::cli`. Any code
//! review that introduces such an import blocks the merge.

// Submodules added in Phase 2: newtypes, track, camelot.
