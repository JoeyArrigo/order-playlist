//! Shared integration-test plumbing for end_to_end / determinism / unresolved tests.

use std::path::PathBuf;
use tempfile::TempDir;

use order_playlist::adapters::Cache;
use order_playlist::cli::ResolvedArgs;
use order_playlist::run::{run, ExitCode, RunDeps, RunReport};

use super::in_memory::{InMemoryFeatureSource, InMemoryResolver};

#[allow(dead_code)]
pub struct SmallPartyRun {
    pub dir: TempDir,
    pub output: PathBuf,
    pub unresolved: PathBuf,
    pub exit: ExitCode,
    pub report: RunReport,
}

/// Run the small_party fixture through the orchestration with the
/// given seed, returning paths to the produced artifacts.
#[allow(dead_code)]
pub async fn run_small_party_with_seed(seed: u64) -> SmallPartyRun {
    run_small_party_with_seed_and_skip_and_window(seed, &[], 4).await
}

/// Same as above, but force the named queries to be unresolved by
/// excluding them from both the cache and the in-memory resolver map.
/// Used by AC5.2 tests to test determinism of unresolved.csv.
#[allow(dead_code)]
pub async fn run_small_party_with_seed_and_skip(seed: u64, skip_titles: &[&str]) -> SmallPartyRun {
    run_small_party_with_seed_and_skip_and_window(seed, skip_titles, 4).await
}

/// Run with custom artist_window (for AC5.3 test).
pub async fn run_small_party_with_seed_and_skip_and_window(
    seed: u64,
    skip_titles: &[&str],
    artist_window: u8,
) -> SmallPartyRun {
    let dir = TempDir::new().unwrap();
    let input = PathBuf::from("tests/fixtures/small_party.csv");
    let output = dir.path().join("out.csv");
    let unresolved = dir.path().join("unresolved.csv");
    let cache_path = dir.path().join("cache.json");

    // Load the full cache to extract entries.
    let full_cache = Cache::load(std::path::Path::new(
        "tests/fixtures/small_party.cache.json",
    ))
    .unwrap();

    // Build a filtered cache that excludes skipped titles.
    let mut filtered_cache = Cache::load(&cache_path).unwrap();
    for (q, id) in full_cache.all_resolutions() {
        if !skip_titles.contains(&q.title.as_str()) {
            filtered_cache.put_resolution(q.clone(), id.clone());
        }
    }
    for (id, feat) in full_cache.all_features() {
        filtered_cache.put_features(id.clone(), feat.clone());
    }
    filtered_cache.save_atomic().unwrap();

    let cache = Cache::load(&cache_path).unwrap();

    // Resolver only knows about non-skipped titles (same as cache).
    let resolver_pairs: Vec<_> = cache
        .all_resolutions()
        .map(|(q, id)| (q.clone(), id.clone()))
        .collect();
    let resolver = InMemoryResolver::new(resolver_pairs);

    let features =
        InMemoryFeatureSource::new(cache.all_features().map(|(id, f)| (id.clone(), f.clone())));

    let args = ResolvedArgs {
        input,
        output: output.clone(),
        unresolved: unresolved.clone(),
        cache: cache_path.clone(),
        seed,
        seed_was_supplied: true,
        artist_window,
        verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };

    let cache = std::sync::Arc::new(tokio::sync::Mutex::new(Cache::load(&cache_path).unwrap()));

    let (exit, report) = run(
        args,
        RunDeps {
            resolver: Box::new(resolver),
            feature_source: Box::new(features),
            cache,
        },
    )
    .await
    .unwrap();

    SmallPartyRun {
        dir,
        output,
        unresolved,
        exit,
        report,
    }
}
