//! Generate `tests/fixtures/small_party.cache.json` from a fixed seed.
//! Run: `cargo run --bin build_small_party_cache`.
//! The output is committed to the repo so `cargo test` is hermetic.

use order_playlist::adapters::Cache;
use order_playlist::domain::{
    Bpm, Mode, Normalized, PitchClass, TrackFeatures, TrackId, TrackQuery,
};
use std::path::PathBuf;

fn main() {
    let queries = [
        ("Get Lucky", "Daft Punk"),
        ("One More Time", "Daft Punk"),
        ("Harder Better Faster Stronger", "Daft Punk"),
        ("Around the World", "Daft Punk"),
        ("Take a Chance on Me", "ABBA"),
        ("Dancing Queen", "ABBA"),
        ("Mamma Mia", "ABBA"),
        ("SOS", "ABBA"),
        ("Levitating", "Dua Lipa"),
        ("Don't Start Now", "Dua Lipa"),
    ];

    let path = PathBuf::from("tests/fixtures/small_party.cache.json");
    // Start fresh — explicitly overwrite the file rather than merging.
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }
    let mut cache = Cache::load(&path).unwrap();

    for (i, (title, artist)) in queries.iter().enumerate() {
        let q = TrackQuery::new(*title, *artist);
        let id = TrackId::new(format!("FAKE{:09}", i + 1));
        // Use Bpm/PitchClass/Normalized constructors so values
        // round-trip identically through serde.
        let features = TrackFeatures {
            tempo: Bpm::new(110.0 + 5.0 * (i as f32)).unwrap(),
            key: PitchClass::new((i as u8) % 12).unwrap(),
            mode: if i % 2 == 0 { Mode::Major } else { Mode::Minor },
            energy: Normalized::clamp(0.3 + 0.06 * (i as f32)),
            danceability: Normalized::clamp(0.5),
            valence: Normalized::clamp(0.5),
            loudness: -10.0,
            acousticness: Normalized::clamp(0.2),
            instrumentalness: Normalized::clamp(0.05),
            liveness: Normalized::clamp(0.1),
            speechiness: Normalized::clamp(0.05),
        };
        cache.put_resolution(q, id.clone());
        cache.put_features(id, features);
    }

    cache.save_atomic().unwrap();
    println!("wrote {}", path.display());
}
