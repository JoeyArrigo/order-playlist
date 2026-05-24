// pattern: Functional Core — pure algorithm, no IO/async

//! Shared test support helpers for the algo module.

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
pub mod helpers {
    use crate::domain::{
        Bpm, Mode, Normalized, PitchClass, Track, TrackFeatures, TrackId, TrackQuery,
    };

    /// Build n deterministic Tracks with seeded random features and a small artist pool.
    ///
    /// Produces tracks with:
    /// - Artists distributed across a small pool (5 artists via i % 5) to naturally exercise
    ///   the artist_clash branch in delta_cost during proptest runs.
    /// - Feature variance scaled by ~100× using ChaCha20Rng for deterministic but varied
    ///   tempo [60, 180], energy/danceability/etc fully spanning [0, 1].
    pub fn synthetic_tracks(n: usize, seed: u64) -> Vec<Track> {
        use rand::{Rng, SeedableRng};
        use rand_chacha::ChaCha20Rng;

        let mut tracks = Vec::with_capacity(n);
        let mut rng = ChaCha20Rng::seed_from_u64(seed);

        for i in 0..n {
            let artist_idx = i % 5;
            let query = TrackQuery::new(format!("Track {}", i), format!("Artist {}", artist_idx));
            let id = TrackId::new(format!("id-{}", i));

            let mut features = TrackFeatures::neutral();

            // Generate tempo in [60, 180] using rng
            let tempo_raw = rng.next_u32() % 121; // 0..121
            let tempo_val = 60.0 + tempo_raw as f32;
            features.tempo = Bpm::new(tempo_val).unwrap();

            let key_val = rng.next_u32() as u8 % 12;
            features.key = PitchClass::new(key_val).unwrap();

            features.mode = if (rng.next_u32() & 1) == 0 {
                Mode::Major
            } else {
                Mode::Minor
            };

            // Generate energy fully across [0, 1]
            let energy_raw = rng.next_u32();
            let energy_val = (energy_raw as f32) / (u32::MAX as f32 + 1.0);
            features.energy = Normalized::clamp(energy_val);

            tracks.push(Track {
                query,
                id,
                features,
            });
        }

        tracks
    }

    /// Build n synthetic tracks with specific artists distributed round-robin.
    ///
    /// Used in proptest inputs and artist-spacing tests. Each position i gets
    /// artist (i % n_artists), ensuring deterministic but varied distribution.
    pub fn synthetic_tracks_with_artists(n: usize, n_artists: usize, seed: u64) -> Vec<Track> {
        use rand::{Rng, SeedableRng};
        use rand_chacha::ChaCha20Rng;

        let mut tracks = Vec::with_capacity(n);
        let mut rng = ChaCha20Rng::seed_from_u64(seed);

        for i in 0..n {
            let artist_idx = i % n_artists;
            let query = TrackQuery::new(format!("Track {}", i), format!("Artist {}", artist_idx));
            let id = TrackId::new(format!("id-{}", i));

            let mut features = TrackFeatures::neutral();

            // Generate tempo in [60, 180] using rng
            let tempo_raw = rng.next_u32() % 121; // 0..121
            let tempo_val = 60.0 + tempo_raw as f32;
            features.tempo = Bpm::new(tempo_val).unwrap();

            let key_val = rng.next_u32() as u8 % 12;
            features.key = PitchClass::new(key_val).unwrap();

            features.mode = if (rng.next_u32() & 1) == 0 {
                Mode::Major
            } else {
                Mode::Minor
            };

            // Generate energy fully across [0, 1]
            let energy_raw = rng.next_u32();
            let energy_val = (energy_raw as f32) / (u32::MAX as f32 + 1.0);
            features.energy = Normalized::clamp(energy_val);

            tracks.push(Track {
                query,
                id,
                features,
            });
        }

        tracks
    }
}

#[cfg(test)]
pub use helpers::{synthetic_tracks, synthetic_tracks_with_artists};
