#![cfg(all(feature = "reccobeats", feature = "live-network"))]

use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

use order_playlist::adapters::{Cache, FeatureSource, ReccoBeatsFeatures};
use order_playlist::domain::TrackId;

#[tokio::test]
async fn fetches_features_for_known_isrc() {
    let dir = TempDir::new().unwrap();
    let cache = Arc::new(Mutex::new(
        Cache::load(&dir.path().join("cache.json")).unwrap(),
    ));

    let src = ReccoBeatsFeatures::new(cache).unwrap();
    let id = TrackId::new("USQX91300120"); // Daft Punk - Get Lucky
    let result = src.features_for(&[id.clone()]).await;
    assert_eq!(result.len(), 1);

    match &result[0].1 {
        Some(features) => {
            assert!(features.tempo.get() > 0.0);
            assert!(features.energy.get() >= 0.0 && features.energy.get() <= 1.0);
        }
        None => panic!("expected features for known ISRC; ReccoBeats may have changed behavior — re-run Task 1 spike"),
    }
}
