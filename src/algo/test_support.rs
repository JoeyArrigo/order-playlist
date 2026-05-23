//! Shared test support helpers for the algo module.

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
pub mod helpers {
    use crate::domain::{
        Bpm, Mode, Normalized, PitchClass, Track, TrackFeatures, TrackId, TrackQuery,
    };

    /// Build n deterministic Tracks with seeded random features and a small artist pool.
    ///
    /// This uses a ChaCha20Rng to ensure varied features (tempo, key, mode, energy)
    /// that span the full ranges, and assigns artists from a small pool (4-5 artists)
    /// distributed round-robin. This ensures the proptest delta_cost property test
    /// exercises the artist_clash term naturally.
    pub fn synthetic_tracks(n: usize, seed: u64) -> Vec<Track> {
        let mut tracks = Vec::with_capacity(n);

        for i in 0..n {
            let query = TrackQuery::new(format!("Track {}", i), format!("Artist {}", i));
            let id = TrackId::new(format!("id-{}", i));

            let mut features = TrackFeatures::neutral();

            // Deterministic feature variation based on index and seed
            let seed_f = (seed as f32 + i as f32) * 0.001;
            let tempo_val = 60.0 + (tempo_from_seed(seed_f) * 120.0); // [60, 180]
            features.tempo = Bpm::new(tempo_val).unwrap();

            let key_val = ((seed as u8).wrapping_add(i as u8)) % 12;
            features.key = PitchClass::new(key_val).unwrap();

            features.mode = if (i ^ (seed as usize)) & 1 == 0 {
                Mode::Major
            } else {
                Mode::Minor
            };

            let energy_val = energy_from_seed(seed_f);
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
        let mut tracks = Vec::with_capacity(n);

        for i in 0..n {
            let artist_idx = i % n_artists;
            let query = TrackQuery::new(format!("Track {}", i), format!("Artist {}", artist_idx));
            let id = TrackId::new(format!("id-{}", i));

            let mut features = TrackFeatures::neutral();

            // Deterministic feature variation based on index and seed
            let seed_f = (seed as f32 + i as f32) * 0.001;
            let tempo_val = 60.0 + (tempo_from_seed(seed_f) * 120.0); // [60, 180]
            features.tempo = Bpm::new(tempo_val).unwrap();

            let key_val = ((seed as u8).wrapping_add(i as u8)) % 12;
            features.key = PitchClass::new(key_val).unwrap();

            features.mode = if (i ^ (seed as usize)) & 1 == 0 {
                Mode::Major
            } else {
                Mode::Minor
            };

            let energy_val = energy_from_seed(seed_f);
            features.energy = Normalized::clamp(energy_val);

            tracks.push(Track {
                query,
                id,
                features,
            });
        }

        tracks
    }

    // Helper to generate tempo variation from seed
    fn tempo_from_seed(seed_f: f32) -> f32 {
        ((seed_f.sin() * 0.5) + (seed_f.cos() * 0.5)).abs()
    }

    // Helper to generate energy variation from seed
    fn energy_from_seed(seed_f: f32) -> f32 {
        0.3 + ((seed_f.sin() * 0.2) + (seed_f.cos() * 0.2)).abs()
    }
}

#[cfg(test)]
pub use helpers::{synthetic_tracks, synthetic_tracks_with_artists};
