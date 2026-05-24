mod support;

use std::path::PathBuf;
use tempfile::TempDir;

use order_playlist::cli::ResolvedArgs;
use order_playlist::domain::{
    Bpm, Mode, Normalized, PitchClass, TrackFeatures, TrackId, TrackQuery,
};
use order_playlist::run::{run, ExitCode, RunDeps};

use support::in_memory::{InMemoryFeatureSource, InMemoryResolver};

#[tokio::test]
async fn partial_unresolved_exits_zero_and_writes_sidecar() {
    let dir = TempDir::new().unwrap();
    let input = PathBuf::from("tests/fixtures/with_bad_rows.csv");
    let unresolved_path = dir.path().join("unresolved.csv");

    // Resolver knows 5 of the 8 queries; the other 3 → Unresolved.
    let resolver = InMemoryResolver::new([
        (
            TrackQuery::new("Get Lucky", "Daft Punk"),
            TrackId::new("FAKE001"),
        ),
        (
            TrackQuery::new("Dancing Queen", "ABBA"),
            TrackId::new("FAKE002"),
        ),
        (
            TrackQuery::new("Levitating", "Dua Lipa"),
            TrackId::new("FAKE003"),
        ),
        (
            TrackQuery::new("One More Time", "Daft Punk"),
            TrackId::new("FAKE004"),
        ),
        (TrackQuery::new("SOS", "ABBA"), TrackId::new("FAKE005")),
    ]);
    let make_features = || TrackFeatures {
        tempo: Bpm::new(120.0).unwrap(),
        key: PitchClass::new(0).unwrap(),
        mode: Mode::Major,
        energy: Normalized::clamp(0.5),
        danceability: Normalized::clamp(0.5),
        valence: Normalized::clamp(0.5),
        loudness: -10.0,
        acousticness: Normalized::clamp(0.5),
        instrumentalness: Normalized::clamp(0.0),
        liveness: Normalized::clamp(0.0),
        speechiness: Normalized::clamp(0.0),
    };
    let features = InMemoryFeatureSource::new([
        (TrackId::new("FAKE001"), make_features()),
        (TrackId::new("FAKE002"), make_features()),
        (TrackId::new("FAKE003"), make_features()),
        (TrackId::new("FAKE004"), make_features()),
        (TrackId::new("FAKE005"), make_features()),
    ]);

    let args = ResolvedArgs {
        input,
        output: dir.path().join("out.csv"),
        unresolved: unresolved_path.clone(),
        cache: dir.path().join("cache.json"),
        seed: 42,
        seed_was_supplied: true,
        artist_window: 4,
        verbose: 0,
        musicbrainz_contact: "test@example.com".into(),
    };
    let cache_for_run = std::sync::Arc::new(tokio::sync::Mutex::new(
        order_playlist::adapters::Cache::load(&dir.path().join("cache.json")).unwrap(),
    ));

    let (exit, _report) = run(
        args,
        RunDeps {
            resolver: Box::new(resolver),
            feature_source: Box::new(features),
            cache: cache_for_run,
        },
    )
    .await
    .unwrap();

    // AC4.2: ≥ 1 resolved → exit 0 even with unresolved.
    assert_eq!(exit, ExitCode::Success);

    // AC4.1: sidecar exists with title,artist,reason columns.
    let sidecar = std::fs::read_to_string(&unresolved_path).unwrap();
    assert!(sidecar.starts_with("title,artist,reason\n"));
    assert_eq!(sidecar.lines().count(), 4); // header + 3 unresolved

    // AC4.5: re-feed the sidecar as input — read_input should accept it.
    let queries = order_playlist::adapters::read_input(&unresolved_path).unwrap();
    assert_eq!(queries.len(), 3);
}

#[tokio::test]
async fn all_unresolved_exits_5() {
    let dir = TempDir::new().unwrap();
    let input = PathBuf::from("tests/fixtures/small_party.csv");

    // Resolver knows nothing.
    let resolver = InMemoryResolver::new([]);
    let features = InMemoryFeatureSource::new([]);

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
    let cache_for_run = std::sync::Arc::new(tokio::sync::Mutex::new(
        order_playlist::adapters::Cache::load(&dir.path().join("cache.json")).unwrap(),
    ));

    let (exit, _report) = run(
        args,
        RunDeps {
            resolver: Box::new(resolver),
            feature_source: Box::new(features),
            cache: cache_for_run,
        },
    )
    .await
    .unwrap();

    // AC4.4: all unresolved → exit 5.
    assert_eq!(exit, ExitCode::NothingResolved);
}
