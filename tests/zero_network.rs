mod support;

use std::path::PathBuf;
use tempfile::TempDir;

use playlistize::cli::ResolvedArgs;
use playlistize::run::{run, ExitCode, RunDeps};

use support::in_memory::{PanicOnCallFeatureSource, PanicOnCallResolver};

#[tokio::test]
async fn warm_cache_does_not_invoke_adapters() {
    let dir = TempDir::new().unwrap();
    let input = PathBuf::from("tests/fixtures/small_party.csv");
    let cache = dir.path().join("cache.json");
    // Copy the pre-warmed cache from Task 7's fixture.
    std::fs::copy("tests/fixtures/small_party.cache.json", &cache).unwrap();

    let args = ResolvedArgs {
        input,
        output: dir.path().join("out.csv"),
        unresolved: dir.path().join("unresolved.csv"),
        cache,
        seed: 42,
        seed_was_supplied: true,
        artist_window: 4,
        verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };

    // AC7.1: the panic-on-call adapters MUST NOT be invoked.
    // If `run` reaches into the resolver or feature_source,
    // the panic fails the test with a clear message.
    let (exit, _report) = run(
        args,
        RunDeps {
            resolver: Box::new(PanicOnCallResolver),
            feature_source: Box::new(PanicOnCallFeatureSource),
        },
    )
    .await
    .unwrap();

    assert_eq!(exit, ExitCode::Success);
}
