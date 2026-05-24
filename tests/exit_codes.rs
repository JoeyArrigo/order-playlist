mod support;

use std::path::PathBuf;
use tempfile::TempDir;

use playlistize::cli::ResolvedArgs;
use playlistize::run::{run, ExitCode, RunDeps};

use support::in_memory::{InMemoryFeatureSource, InMemoryResolver};

#[tokio::test]
async fn nonexistent_input_returns_input_error() {
    let dir = TempDir::new().unwrap();
    let args = ResolvedArgs {
        input: PathBuf::from("/nonexistent/input.csv"),
        output: dir.path().join("out.csv"),
        unresolved: dir.path().join("unresolved.csv"),
        cache: dir.path().join("cache.json"),
        seed: 42,
        seed_was_supplied: true,
        artist_window: 4,
        verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };

    let cache = std::sync::Arc::new(tokio::sync::Mutex::new(
        playlistize::adapters::Cache::load(&dir.path().join("cache.json")).unwrap(),
    ));

    let (exit, report) = run(
        args,
        RunDeps {
            resolver: Box::new(InMemoryResolver::new([])),
            feature_source: Box::new(InMemoryFeatureSource::new([])),
            cache,
        },
    )
    .await
    .unwrap();

    assert_eq!(exit, ExitCode::InputError);
    assert!(
        !report.message.is_empty(),
        "error message should describe the failure"
    );
}

#[tokio::test]
async fn missing_header_returns_input_error() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("bad_header.csv");

    // Create a CSV with missing 'title' or 'artist' column
    std::fs::write(&input, "foo,bar\nx,y\n").unwrap();

    let args = ResolvedArgs {
        input,
        output: dir.path().join("out.csv"),
        unresolved: dir.path().join("unresolved.csv"),
        cache: dir.path().join("cache.json"),
        seed: 42,
        seed_was_supplied: true,
        artist_window: 4,
        verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };

    let cache = std::sync::Arc::new(tokio::sync::Mutex::new(
        playlistize::adapters::Cache::load(&dir.path().join("cache.json")).unwrap(),
    ));

    let (exit, report) = run(
        args,
        RunDeps {
            resolver: Box::new(InMemoryResolver::new([])),
            feature_source: Box::new(InMemoryFeatureSource::new([])),
            cache,
        },
    )
    .await
    .unwrap();

    assert_eq!(exit, ExitCode::InputError);
    assert!(
        report.message.contains("missing")
            || report.message.contains("title")
            || report.message.contains("artist"),
        "error should mention missing required columns; got: {}",
        report.message
    );
}

#[tokio::test]
async fn zero_rows_returns_input_error() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("header_only.csv");

    // Create a CSV with header but no data rows
    std::fs::write(&input, "title,artist\n").unwrap();

    let args = ResolvedArgs {
        input,
        output: dir.path().join("out.csv"),
        unresolved: dir.path().join("unresolved.csv"),
        cache: dir.path().join("cache.json"),
        seed: 42,
        seed_was_supplied: true,
        artist_window: 4,
        verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };

    let cache = std::sync::Arc::new(tokio::sync::Mutex::new(
        playlistize::adapters::Cache::load(&dir.path().join("cache.json")).unwrap(),
    ));

    let (exit, report) = run(
        args,
        RunDeps {
            resolver: Box::new(InMemoryResolver::new([])),
            feature_source: Box::new(InMemoryFeatureSource::new([])),
            cache,
        },
    )
    .await
    .unwrap();

    assert_eq!(exit, ExitCode::InputError);
    assert!(
        report.message.contains("zero") || report.message.contains("no rows"),
        "error should mention missing rows; got: {}",
        report.message
    );
}

#[test]
fn corrupt_cache_load_or_exit_returns_cache_error() {
    let dir = TempDir::new().unwrap();
    let cache_path = dir.path().join("cache.json");

    // Write invalid JSON to the cache file
    std::fs::write(&cache_path, b"{ not valid json").unwrap();

    let result = playlistize::load_cache_or_exit_code(&cache_path);
    match result {
        Err((exit, _msg)) => assert_eq!(
            exit,
            ExitCode::CacheError,
            "corrupt cache should map to CacheError exit code"
        ),
        Ok(_) => panic!("expected CacheError for corrupt cache"),
    }
}

#[test]
fn version_mismatch_cache_load_or_exit_returns_cache_error() {
    let dir = TempDir::new().unwrap();
    let cache_path = dir.path().join("cache.json");

    // Write valid JSON but with wrong version
    let invalid_cache = serde_json::json!({
        "version": 999,
        "resolutions": [],
        "features": {}
    });
    std::fs::write(&cache_path, serde_json::to_string(&invalid_cache).unwrap()).unwrap();

    let result = playlistize::load_cache_or_exit_code(&cache_path);
    match result {
        Err((exit, _msg)) => assert_eq!(
            exit,
            ExitCode::CacheError,
            "version mismatch should map to CacheError exit code"
        ),
        Ok(_) => panic!("expected CacheError for version mismatch"),
    }
}
