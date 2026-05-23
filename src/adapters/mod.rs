//! Impure adapter shell — file IO, HTTP, JSON cache.
//!
//! Adapter trait definitions and feature-gated implementations live here.
//! The default v1 implementations (`musicbrainz`, `reccobeats`) are gated
//! by Cargo features of the same name. Adding a new provider means adding
//! a new feature flag and a new impl; nothing in `algo/` or `domain/`
//! should change.

// Submodules added in later phases: csv_io, cache (Phase 4),
// musicbrainz (Phase 5), reccobeats (Phase 6).
