mod support;

use std::path::PathBuf;
use tempfile::TempDir;

use order_playlist::adapters::Cache;
use order_playlist::cli::ResolvedArgs;
use order_playlist::run::{run, ExitCode, RunDeps};

use support::in_memory::{InMemoryFeatureSource, InMemoryResolver};

#[tokio::test]
async fn end_to_end_warm_cache_produces_output() {
    let dir = TempDir::new().unwrap();
    let input = PathBuf::from("tests/fixtures/small_party.csv");
    let output = dir.path().join("out.csv");
    let unresolved = dir.path().join("unresolved.csv");
    let cache = dir.path().join("cache.json");
    // Copy the pre-warmed cache into the temp dir.
    std::fs::copy("tests/fixtures/small_party.cache.json", &cache).unwrap();

    // Build in-memory deps that mirror the cache contents.
    let resolver = build_resolver_from_cache(&cache).await;
    let feature_source = build_features_from_cache(&cache).await;

    let args = ResolvedArgs {
        input,
        output: output.clone(),
        unresolved,
        cache,
        seed: 42,
        seed_was_supplied: true,
        artist_window: 4,
        verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };

    let cache_for_run = std::sync::Arc::new(tokio::sync::Mutex::new(
        Cache::load(&dir.path().join("cache.json")).unwrap(),
    ));

    let (exit, _report) = run(
        args,
        RunDeps {
            resolver: Box::new(resolver),
            feature_source: Box::new(feature_source),
            cache: cache_for_run,
        },
    )
    .await
    .unwrap();

    assert_eq!(exit, ExitCode::Success);

    let out = std::fs::read_to_string(&output).unwrap();
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(
        lines[0],
        "position,title,artist,tempo,key,mode,energy,danceability,valence,loudness,isrc"
    );
    assert_eq!(lines.len(), 11); // header + 10 rows
}

async fn build_resolver_from_cache(cache_path: &std::path::Path) -> InMemoryResolver {
    // Uses the `Cache::all_resolutions` accessor added by Task 4b.
    let cache = Cache::load(cache_path).unwrap();
    InMemoryResolver::new(
        cache
            .all_resolutions()
            .map(|(q, id)| (q.clone(), id.clone())),
    )
}

async fn build_features_from_cache(cache_path: &std::path::Path) -> InMemoryFeatureSource {
    // Uses the `Cache::all_features` accessor added by Task 4b.
    let cache = Cache::load(cache_path).unwrap();
    InMemoryFeatureSource::new(cache.all_features().map(|(id, f)| (id.clone(), f.clone())))
}
