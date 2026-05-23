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
}
