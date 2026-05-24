mod support;

use std::path::PathBuf;
use tempfile::TempDir;

use order_playlist::cli::ResolvedArgs;
use order_playlist::run::{run, ExitCode, RunDeps};

use support::in_memory::{
    ExhaustingFeatureSource, ExhaustingResolver, InMemoryFeatureSource, InMemoryResolver,
};

fn args_for(dir: &TempDir, input: PathBuf) -> ResolvedArgs {
    ResolvedArgs {
        input,
        output: dir.path().join("out.csv"),
        unresolved: dir.path().join("unresolved.csv"),
        cache: dir.path().join("cache.json"),
        seed: 42,
        seed_was_supplied: true,
        artist_window: 4,
        verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    }
}

fn empty_cache(
    dir: &TempDir,
) -> std::sync::Arc<tokio::sync::Mutex<order_playlist::adapters::Cache>> {
    std::sync::Arc::new(tokio::sync::Mutex::new(
        order_playlist::adapters::Cache::load(&dir.path().join("cache.json")).unwrap(),
    ))
}

#[tokio::test]
async fn resolver_exhaustion_yields_network_exhausted_when_nothing_resolved() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.csv");
    std::fs::write(&input, "title,artist\nFoo,Bar\n").unwrap();

    let (exit, report) = run(
        args_for(&dir, input),
        RunDeps {
            resolver: Box::new(ExhaustingResolver),
            feature_source: Box::new(InMemoryFeatureSource::new([])),
            cache: empty_cache(&dir),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        exit,
        ExitCode::NetworkExhausted,
        "resolver exhaustion with zero resolutions should surface as NetworkExhausted, got {:?} (msg: {})",
        exit,
        report.message,
    );
}

#[tokio::test]
async fn feature_source_exhaustion_yields_network_exhausted_when_nothing_resolved() {
    use order_playlist::domain::{TrackId, TrackQuery};

    let dir = TempDir::new().unwrap();
    let input = dir.path().join("in.csv");
    std::fs::write(&input, "title,artist\nFoo,Bar\n").unwrap();

    let resolver = InMemoryResolver::new([(
        TrackQuery::new("Foo", "Bar"),
        TrackId::new("USTEST0000001".to_string()),
    )]);

    let (exit, report) = run(
        args_for(&dir, input),
        RunDeps {
            resolver: Box::new(resolver),
            feature_source: Box::new(ExhaustingFeatureSource),
            cache: empty_cache(&dir),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        exit,
        ExitCode::NetworkExhausted,
        "feature-source exhaustion with zero resolved tracks should surface as NetworkExhausted, got {:?} (msg: {})",
        exit,
        report.message,
    );
}
