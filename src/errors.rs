//! Structured error types. Concrete variants land in later phases:
//! `InputError` (Phase 4), `CacheError` (Phase 4), `AdapterError` (Phases 5–6).
//!
//! Each error type uses `thiserror::Error` and derives
//! `miette::Diagnostic` for user-facing source spans / help text.
