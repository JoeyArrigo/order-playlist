mod support;

use playlistize::adapters::FeatureSource;
use playlistize::domain::{Bpm, Mode, Normalized, PitchClass, TrackFeatures, TrackId};
use support::in_memory::InMemoryFeatureSource;

#[tokio::test]
async fn in_memory_features_returns_known() {
    let id = TrackId::new("USQX91300120");
    let f = TrackFeatures {
        tempo: Bpm::DEFAULT_120,
        key: PitchClass::C,
        mode: Mode::Major,
        energy: Normalized::HALF,
        danceability: Normalized::HALF,
        valence: Normalized::HALF,
        loudness: -10.0,
        acousticness: Normalized::HALF,
        instrumentalness: Normalized::HALF,
        liveness: Normalized::HALF,
        speechiness: Normalized::HALF,
    };
    let src = InMemoryFeatureSource::new([(id.clone(), f.clone())]);
    let result = src.features_for(std::slice::from_ref(&id)).await;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, id);
    assert!(result[0].1.is_some());
    assert_eq!(result[0].1.as_ref().unwrap().tempo.get(), 120.0);
}
