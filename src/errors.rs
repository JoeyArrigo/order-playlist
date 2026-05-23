//! Structured error types. Concrete variants land in later phases:
//! `InputError` (Phase 4), `CacheError` (Phase 4), `AdapterError` (Phases 5–6).
//!
//! Each error type uses `thiserror::Error` and derives
//! `miette::Diagnostic` for user-facing source spans / help text.

use std::path::PathBuf;

/// Error type for input CSV processing failures.
///
/// Each variant includes source paths and/or line numbers for diagnostic output.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum InputError {
    /// Input file does not exist at the given path.
    #[error("input file not found: {path}")]
    #[diagnostic(
        code(playlistize::input::not_found),
        help("verify the path exists and is readable")
    )]
    NotFound { path: PathBuf },

    /// Output parent directory does not exist.
    #[error("output parent directory does not exist: {parent}")]
    #[diagnostic(
        code(playlistize::input::missing_parent),
        help("create the directory before running, or choose a path whose parent exists")
    )]
    MissingParentDir { parent: PathBuf },

    /// Required CSV column(s) are missing from the header.
    #[error("missing required column(s): {missing:?}")]
    #[diagnostic(
        code(playlistize::input::missing_column),
        help("input CSV must have at minimum 'title' and 'artist' columns")
    )]
    MissingColumn {
        missing: Vec<String>,
        #[source_code]
        header_src: String,
        #[label("header line")]
        span: miette::SourceSpan,
    },

    /// Input CSV has no data rows (header only or empty file).
    #[error("input contained zero data rows")]
    #[diagnostic(
        code(playlistize::input::no_rows),
        help("the file has a valid header but no tracks; add at least one row")
    )]
    NoRows { path: PathBuf },

    /// CSV parsing error at a specific line.
    #[error("CSV parse error at line {line}: {message}")]
    #[diagnostic(code(playlistize::input::csv_parse))]
    Csv {
        line: u64,
        message: String,
        #[source]
        source: csv::Error,
    },

    /// IO error reading input or writing output.
    #[error("IO error reading {path}")]
    #[diagnostic(code(playlistize::input::io))]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Error type for cache file processing failures.
///
/// Includes version mismatches, corruption, and IO errors with full diagnostics.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum CacheError {
    /// Cache schema version does not match the current version.
    #[error("cache version mismatch: file has version {found}, expected {expected}")]
    #[diagnostic(
        code(playlistize::cache::version_mismatch),
        help(
            "delete the cache file or upgrade `playlistize`; cache schema changed between versions"
        )
    )]
    VersionMismatch { found: u32, expected: u32 },

    /// Cache file is corrupted or invalid JSON.
    #[error("cache file is corrupt: {message}")]
    #[diagnostic(
        code(playlistize::cache::corrupt),
        help("delete the cache file and rerun; resolved features will be re-fetched")
    )]
    Corrupt {
        message: String,
        #[source]
        source: serde_json::Error,
    },

    /// IO error reading or writing the cache file.
    #[error("IO error on cache file {path}")]
    #[diagnostic(code(playlistize::cache::io))]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Error type for ReccoBeats audio-features failures.
///
/// Captures the error kind, the failing ID batch, and an optional underlying
/// reqwest error (for network errors, not for application-level failures).
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("ReccoBeats {kind:?} for {ids:?}")]
#[diagnostic(code(playlistize::adapter::reccobeats::error))]
pub struct ReccoBeatsError {
    /// Classification of the failure (network, parse, throttled).
    pub kind: ReccoBeatsErrorKind,
    /// The IDs that failed.
    pub ids: Vec<String>,
    /// Underlying reqwest error, if this was a network/HTTP failure.
    #[source]
    pub source: Option<reqwest::Error>,
}

/// Classification of ReccoBeats audio-features failures.
///
/// Used to distinguish transient errors (network, throttled) from
/// semantic failures (parse errors).
#[derive(Debug, Clone)]
pub enum ReccoBeatsErrorKind {
    /// Network error: connection failed, timeout, or other I/O issue.
    Network,
    /// Parse error: JSON deserialization failed.
    Parse,
    /// Throttled: HTTP 429 received (transient, retryable).
    Throttled,
}

/// Error type for adapter (resolver) failures.
///
/// Groups errors from multiple adapter backends (MusicBrainz, ReccoBeats, etc.)
/// under a single enum so orchestration can handle them uniformly.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum AdapterError {
    /// MusicBrainz-specific resolution failure.
    #[error("MusicBrainz error: {0}")]
    #[diagnostic(code(playlistize::adapter::musicbrainz))]
    MusicBrainz(#[from] MusicBrainzError),

    /// ReccoBeats-specific audio-features failure.
    #[error("ReccoBeats error: {0}")]
    #[diagnostic(code(playlistize::adapter::reccobeats))]
    ReccoBeats(#[from] ReccoBeatsError),

    /// Rate limiting encountered on an adapter endpoint.
    ///
    /// This may come from MusicBrainz (HTTP 503) or other providers.
    /// Exhausted retries after rate limiting.
    #[error("rate limited; exhausted retries on {endpoint}")]
    #[diagnostic(
        code(playlistize::adapter::rate_limited),
        help("re-run later; consider authenticated MusicBrainz access for higher quota")
    )]
    RateLimited { endpoint: String },
}

/// Error type for MusicBrainz WS2 resolution failures.
///
/// Captures the error kind, the failing query, and an optional underlying
/// reqwest error (for network errors, not for application-level failures like
/// "no candidates found").
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("MusicBrainz {kind:?} for {query:?}")]
#[diagnostic(code(playlistize::adapter::musicbrainz::error))]
pub struct MusicBrainzError {
    /// Classification of the failure (network, parse, rate limit, no candidates).
    pub kind: MusicBrainzErrorKind,
    /// The query that failed.
    pub query: crate::domain::TrackQuery,
    /// Underlying reqwest error, if this was a network/HTTP failure.
    #[source]
    pub source: Option<reqwest::Error>,
}

/// Classification of MusicBrainz resolution failures.
///
/// Used to distinguish transient errors (network, rate limit) from
/// semantic failures (parse errors, no matching candidates).
#[derive(Debug, Clone)]
pub enum MusicBrainzErrorKind {
    /// Network error: connection failed, timeout, or other I/O issue.
    Network,
    /// Parse error: JSON deserialization failed.
    Parse,
    /// Rate limiting: HTTP 503 or 429 received.
    RateLimit,
    /// No matching candidates: search returned empty or all candidates had no ISRCs.
    NoCandidates,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_error_not_found_formats_path() {
        let err = InputError::NotFound {
            path: "/missing".into(),
        };
        let display = err.to_string();
        assert!(
            display.contains("/missing"),
            "path should be visible in error"
        );
    }

    #[test]
    fn input_error_missing_column_constructs() {
        let err = InputError::MissingColumn {
            missing: vec!["artist".into()],
            header_src: "title,foo".into(),
            span: (0usize, 11).into(),
        };
        let display = err.to_string();
        assert!(
            display.contains("artist"),
            "missing column name should be visible"
        );
    }

    #[test]
    fn cache_error_version_mismatch_formats() {
        let err = CacheError::VersionMismatch {
            found: 1,
            expected: 2,
        };
        let display = err.to_string();
        assert!(display.contains("1"), "found version should be visible");
        assert!(display.contains("2"), "expected version should be visible");
    }

    #[test]
    fn adapter_error_musicbrainz_formats_query_title() {
        let mb_err = MusicBrainzError {
            kind: MusicBrainzErrorKind::Network,
            query: crate::domain::TrackQuery::new("Test Song", "Test Artist"),
            source: None,
        };
        let adapter_err: AdapterError = mb_err.into();
        let display = adapter_err.to_string();
        assert!(
            display.contains("Test Song"),
            "query title should appear in error message"
        );
    }

    #[test]
    fn adapter_error_rate_limited_formats() {
        let err = AdapterError::RateLimited {
            endpoint: "musicbrainz.org".into(),
        };
        let display = err.to_string();
        assert!(
            display.contains("musicbrainz.org"),
            "endpoint should be visible in error"
        );
        assert!(display.contains("rate limited"));
    }

    #[test]
    fn musicbrainz_error_with_network_kind() {
        let err = MusicBrainzError {
            kind: MusicBrainzErrorKind::Network,
            query: crate::domain::TrackQuery::new("Title", "Artist"),
            source: None,
        };
        let display = err.to_string();
        assert!(display.contains("Network"));
        assert!(display.contains("Title"));
        assert!(display.contains("Artist"));
    }

    #[test]
    fn musicbrainz_error_with_no_candidates_kind() {
        let err = MusicBrainzError {
            kind: MusicBrainzErrorKind::NoCandidates,
            query: crate::domain::TrackQuery::new("Obscure", "Band"),
            source: None,
        };
        let display = err.to_string();
        assert!(display.contains("NoCandidates"));
        assert!(display.contains("Obscure"));
    }

    #[test]
    fn reccobeats_error_network_kind_constructs() {
        let err = ReccoBeatsError {
            kind: ReccoBeatsErrorKind::Network,
            ids: vec!["USQX91300120".to_string()],
            source: None,
        };
        let display = err.to_string();
        assert!(display.contains("Network"));
        assert!(display.contains("USQX91300120"));
    }

    #[test]
    fn reccobeats_error_parse_kind_constructs() {
        let err = ReccoBeatsError {
            kind: ReccoBeatsErrorKind::Parse,
            ids: vec!["ID1".to_string(), "ID2".to_string()],
            source: None,
        };
        let display = err.to_string();
        assert!(display.contains("Parse"));
        assert!(display.contains("ID1"));
    }

    #[test]
    fn reccobeats_error_throttled_kind_constructs() {
        let err = ReccoBeatsError {
            kind: ReccoBeatsErrorKind::Throttled,
            ids: vec!["USQX91300120".to_string()],
            source: None,
        };
        let display = err.to_string();
        assert!(display.contains("Throttled"));
    }

    #[test]
    fn adapter_error_from_reccobeats_error() {
        let rb_err = ReccoBeatsError {
            kind: ReccoBeatsErrorKind::Network,
            ids: vec!["USQX91300120".to_string()],
            source: None,
        };
        let adapter_err: AdapterError = rb_err.into();
        let display = adapter_err.to_string();
        assert!(display.contains("ReccoBeats error"));
        assert!(display.contains("USQX91300120"));
    }
}
