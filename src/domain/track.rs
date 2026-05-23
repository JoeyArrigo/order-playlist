//! Track types and identifiers for the domain.
//!
//! These types are the only thing the algorithm sees. Adapters collapse
//! all `Option`/`Result` partial-data handling before constructing `Track`.

use crate::domain::{Bpm, Mode, Normalized, PitchClass};
use serde::{Deserialize, Serialize};

/// A search query for a track (title + artist).
/// Derives `Hash` and `Eq` because Phase 4's cache uses `HashMap<TrackQuery, TrackId>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackQuery {
    /// The track title (trimmed of leading/trailing whitespace).
    pub title: String,
    /// The artist name (trimmed of leading/trailing whitespace).
    pub artist: String,
}

impl TrackQuery {
    /// Construct a `TrackQuery` from title and artist, trimming whitespace.
    pub fn new(title: impl Into<String>, artist: impl Into<String>) -> Self {
        Self {
            title: title.into().trim().to_string(),
            artist: artist.into().trim().to_string(),
        }
    }
}

/// An opaque track identifier (e.g., ISRC, Spotify ID).
/// The inner field is `pub(crate)` so downstream modules cannot pattern-match on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackId(pub(crate) String);

impl TrackId {
    /// Construct a `TrackId` from a string. Emits a warn-level tracing event if the string is empty.
    pub fn new(s: impl Into<String>) -> Self {
        let id_string = s.into();
        if id_string.is_empty() {
            tracing::warn!("TrackId created with empty string");
        }
        Self(id_string)
    }

    /// Access the underlying ID string.
    pub fn get(&self) -> &str {
        &self.0
    }
}

/// Numeric audio features extracted from a track (ReccoBeats or equivalent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackFeatures {
    /// Tempo in beats per minute.
    pub tempo: Bpm,
    /// Pitch class (0..=11).
    pub key: PitchClass,
    /// Major or minor mode.
    pub mode: Mode,
    /// Perceived energy [0.0, 1.0].
    pub energy: Normalized,
    /// Danceability [0.0, 1.0].
    pub danceability: Normalized,
    /// Valence (positivity) [0.0, 1.0].
    pub valence: Normalized,
    /// Loudness in dB; typically [-60.0, 0.0], but no clamp enforced.
    pub loudness: f32,
    /// Acousticness [0.0, 1.0].
    pub acousticness: Normalized,
    /// Instrumentalness [0.0, 1.0].
    pub instrumentalness: Normalized,
    /// Liveness [0.0, 1.0].
    pub liveness: Normalized,
    /// Speechiness [0.0, 1.0].
    pub speechiness: Normalized,
}

impl TrackFeatures {
    /// A neutral mid-curve placeholder used by tests and fixtures.
    /// NOT used in production — adapters always populate real values.
    #[cfg(test)]
    pub fn neutral() -> Self {
        Self {
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
        }
    }
}

/// A fully-resolved track: query + ID + audio features.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// The search query used to find this track.
    pub query: TrackQuery,
    /// The opaque identifier assigned by the resolver.
    pub id: TrackId,
    /// Audio features extracted by the feature source.
    pub features: TrackFeatures,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== TrackQuery tests ==========

    #[test]
    fn test_track_query_trims_whitespace() {
        let query = TrackQuery::new("  Hello  ", "World");
        assert_eq!(query.title, "Hello");
        assert_eq!(query.artist, "World");
    }

    #[test]
    fn test_track_query_equality_is_case_sensitive() {
        let q1 = TrackQuery::new("Daft Punk", "Get Lucky");
        let q2 = TrackQuery::new("daft punk", "Get Lucky");
        assert_ne!(q1, q2);
    }

    // ========== TrackId tests ==========

    #[test]
    fn test_track_id_empty_string() {
        let id = TrackId::new("");
        assert_eq!(id.get(), "");
    }

    #[test]
    fn test_track_id_non_empty() {
        let id = TrackId::new("ISRC123");
        assert_eq!(id.get(), "ISRC123");
    }

    // ========== TrackFeatures serde round-trip tests ==========

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_track_features_neutral_round_trip_json() {
        // Bitwise equality acceptable for serde round-trip of constant neutral fixture.
        let original = TrackFeatures::neutral();
        let json = serde_json::to_string(&original).expect("serialize");
        let deserialized: TrackFeatures = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, deserialized);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_track_with_neutral_features_round_trip() {
        // Bitwise equality acceptable for serde round-trip of constant neutral fixture.
        let query = TrackQuery::new("Test Track", "Test Artist");
        let id = TrackId::new("test-id");
        let features = TrackFeatures::neutral();
        let track = Track {
            query,
            id,
            features,
        };

        let json = serde_json::to_string(&track).expect("serialize");
        let deserialized: Track = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(track, deserialized);
    }
}
