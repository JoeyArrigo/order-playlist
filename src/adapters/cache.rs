//! Versioned JSON sidecar that stores resolved IDs and audio features.
//!
//! File schema (incremented on any breaking change):
//! ```json
//! {
//!   "version": 1,
//!   "resolutions": [["title", "artist", "id"], ...],
//!   "features": {"id": { ... }, ...}
//! }
//! ```
//!
//! Atomic-write contract: `Cache::save_atomic()` writes to `path.json.tmp` first,
//! then `std::fs::rename` into place. A mid-run SIGKILL leaves the existing file untouched.

use std::collections::BTreeMap;
use std::path::Path;

use crate::domain::{TrackFeatures, TrackId, TrackQuery};
use crate::errors::CacheError;

/// Schema version. Bump on any breaking change to `resolutions` or `features`.
pub const CACHE_VERSION: u32 = 1;

/// On-disk representation of the cache.
///
/// `resolutions` is stored as a `Vec<(TrackQuery, TrackId)>` to serialize
/// `TrackQuery` as a JSON object. In-memory `Cache` maintains a `BTreeMap`
/// for deterministic iteration order during serialization.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheFile {
    /// Schema version for this cache file.
    pub version: u32,
    /// Mapping from TrackQuery to resolved TrackId.
    /// Stored as Vec to allow TrackQuery to be JSON object; in-memory is BTreeMap.
    #[serde(rename = "resolutions")]
    pub resolutions: Vec<(TrackQuery, TrackId)>,
    /// Audio features indexed by TrackId.
    /// BTreeMap ensures deterministic serialization order.
    pub features: BTreeMap<TrackId, TrackFeatures>,
}

impl Default for CacheFile {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            resolutions: Vec::new(),
            features: BTreeMap::new(),
        }
    }
}

/// In-memory cache with warm-start semantics.
#[derive(Debug)]
pub struct Cache {
    /// File path for atomic save operations.
    path: std::path::PathBuf,
    /// Query → ID resolution cache (BTreeMap for deterministic iteration).
    resolutions: BTreeMap<TrackQuery, TrackId>,
    /// Audio features by ID (BTreeMap for deterministic iteration).
    features: BTreeMap<TrackId, TrackFeatures>,
}

impl Cache {
    /// Load the cache from `path`.
    ///
    /// If the file doesn't exist, returns an empty cache (non-existence is not an error).
    /// If the file exists but is corrupt or version-mismatched, returns an error.
    pub fn load(path: &Path) -> Result<Self, CacheError> {
        if !path.exists() {
            return Ok(Self {
                path: path.to_path_buf(),
                resolutions: BTreeMap::new(),
                features: BTreeMap::new(),
            });
        }

        let bytes = std::fs::read(path).map_err(|e| CacheError::Io {
            path: path.to_path_buf(),
            source: e,
        })?;

        let file: CacheFile = serde_json::from_slice(&bytes).map_err(|e| CacheError::Corrupt {
            message: format!("failed to parse {}: {}", path.display(), e),
            source: e,
        })?;

        if file.version != CACHE_VERSION {
            return Err(CacheError::VersionMismatch {
                found: file.version,
                expected: CACHE_VERSION,
            });
        }

        let resolutions: BTreeMap<TrackQuery, TrackId> = file.resolutions.into_iter().collect();

        Ok(Self {
            path: path.to_path_buf(),
            resolutions,
            features: file.features,
        })
    }

    /// Look up a previously resolved track by query.
    pub fn get_resolution(&self, q: &TrackQuery) -> Option<&TrackId> {
        self.resolutions.get(q)
    }

    /// Insert or update a resolution.
    /// Caller is responsible for calling `save_atomic()` before the process exits.
    pub fn put_resolution(&mut self, q: TrackQuery, id: TrackId) {
        self.resolutions.insert(q, id);
    }

    /// Look up audio features by track ID.
    pub fn get_features(&self, id: &TrackId) -> Option<&TrackFeatures> {
        self.features.get(id)
    }

    /// Insert or update audio features for a track ID.
    /// Caller is responsible for calling `save_atomic()` before the process exits.
    pub fn put_features(&mut self, id: TrackId, features: TrackFeatures) {
        self.features.insert(id, features);
    }

    /// Total number of cached resolutions.
    pub fn resolution_count(&self) -> usize {
        self.resolutions.len()
    }

    /// Total number of cached feature sets.
    pub fn feature_count(&self) -> usize {
        self.features.len()
    }

    /// Atomically persist the cache to disk.
    ///
    /// Steps:
    /// 1. Build a CacheFile from in-memory state.
    /// 2. Serialize to JSON bytes.
    /// 3. Write to `<path>.tmp`.
    /// 4. `fsync` the temp file.
    /// 5. `std::fs::rename(<path>.tmp, <path>)`.
    ///
    /// If step 3 or 4 fails, the existing file at `path` is untouched.
    /// If step 5 fails, the temp file is left behind (caller may clean up).
    pub fn save_atomic(&self) -> Result<(), CacheError> {
        let file = CacheFile {
            version: CACHE_VERSION,
            resolutions: self
                .resolutions
                .iter()
                .map(|(q, id)| (q.clone(), id.clone()))
                .collect(),
            features: self.features.clone(),
        };

        let bytes = serde_json::to_vec_pretty(&file).map_err(|e| CacheError::Corrupt {
            message: format!("failed to serialize cache: {}", e),
            source: e,
        })?;

        let tmp_path = self.path.with_extension(
            self.path
                .extension()
                .map(|e| format!("{}.tmp", e.to_string_lossy()))
                .unwrap_or_else(|| "tmp".to_string()),
        );

        // Write + fsync the tmp file.
        {
            let mut f = std::fs::File::create(&tmp_path).map_err(|e| CacheError::Io {
                path: tmp_path.clone(),
                source: e,
            })?;
            use std::io::Write;
            f.write_all(&bytes).map_err(|e| CacheError::Io {
                path: tmp_path.clone(),
                source: e,
            })?;
            f.sync_all().map_err(|e| CacheError::Io {
                path: tmp_path.clone(),
                source: e,
            })?;
        }

        std::fs::rename(&tmp_path, &self.path).map_err(|e| CacheError::Io {
            path: self.path.clone(),
            source: e,
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Cache::load tests (Task 5) ==========

    #[test]
    fn cache_load_nonexistent_returns_empty() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("nonexistent.json");

        let cache = Cache::load(&path).expect("load");
        assert_eq!(cache.resolution_count(), 0);
        assert_eq!(cache.feature_count(), 0);
    }

    #[test]
    fn cache_load_empty_json_corrupts() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"{}").expect("write");

        let result = Cache::load(temp.path());
        assert!(matches!(result, Err(CacheError::Corrupt { .. })));
    }

    #[test]
    fn cache_load_version_mismatch() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        let json = r#"{"version": 99, "resolutions": [], "features": {}}"#;
        std::fs::write(temp.path(), json).expect("write");

        let result = Cache::load(temp.path());
        match result {
            Err(CacheError::VersionMismatch { found, expected }) => {
                assert_eq!(found, 99);
                assert_eq!(expected, CACHE_VERSION);
            }
            other => panic!("expected VersionMismatch, got {:?}", other),
        }
    }

    #[test]
    fn cache_load_valid_preserves_data() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        let json = r#"{
            "version": 1,
            "resolutions": [
                [{"title": "Song A", "artist": "Artist A"}, "id-1"],
                [{"title": "Song B", "artist": "Artist B"}, "id-2"]
            ],
            "features": {}
        }"#;
        std::fs::write(temp.path(), json).expect("write");

        let cache = Cache::load(temp.path()).expect("load");
        assert_eq!(cache.resolution_count(), 2);

        let q = TrackQuery::new("Song A", "Artist A");
        let id = cache.get_resolution(&q);
        assert_eq!(id.map(|i| i.get()), Some("id-1"));
    }

    // ========== Cache accessor tests (Task 6) ==========

    #[test]
    fn cache_put_get_resolution() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut cache = Cache::load(dir.path().join("dummy.json").as_path()).expect("empty cache");
        let q = TrackQuery::new("Test", "Artist");
        let id = TrackId::new("test-id");

        cache.put_resolution(q.clone(), id.clone());
        let retrieved = cache.get_resolution(&q);

        assert_eq!(retrieved.map(|i| i.get()), Some("test-id"));
    }

    #[test]
    fn cache_put_resolution_overwrites() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut cache = Cache::load(dir.path().join("dummy.json").as_path()).expect("empty cache");
        let q = TrackQuery::new("Test", "Artist");

        cache.put_resolution(q.clone(), TrackId::new("id-1"));
        cache.put_resolution(q.clone(), TrackId::new("id-2"));

        let retrieved = cache.get_resolution(&q);
        assert_eq!(retrieved.map(|i| i.get()), Some("id-2"));
    }

    #[test]
    fn cache_get_resolution_missing_returns_none() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cache = Cache::load(dir.path().join("dummy.json").as_path()).expect("empty cache");
        let q = TrackQuery::new("NonExistent", "Artist");
        assert_eq!(cache.get_resolution(&q), None);
    }

    #[test]
    fn cache_put_get_features() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut cache = Cache::load(dir.path().join("dummy.json").as_path()).expect("empty cache");
        let id = TrackId::new("test-id");
        let features = crate::domain::TrackFeatures {
            tempo: crate::domain::Bpm::new(120.0).expect("valid bpm"),
            key: crate::domain::PitchClass::C,
            mode: crate::domain::Mode::Major,
            energy: crate::domain::Normalized::try_new(0.5).expect("valid norm"),
            danceability: crate::domain::Normalized::try_new(0.6).expect("valid norm"),
            valence: crate::domain::Normalized::try_new(0.7).expect("valid norm"),
            loudness: -5.5,
            acousticness: crate::domain::Normalized::try_new(0.2).expect("valid norm"),
            instrumentalness: crate::domain::Normalized::try_new(0.0).expect("valid norm"),
            liveness: crate::domain::Normalized::try_new(0.1).expect("valid norm"),
            speechiness: crate::domain::Normalized::try_new(0.05).expect("valid norm"),
        };

        cache.put_features(id.clone(), features.clone());
        let retrieved = cache.get_features(&id);

        assert_eq!(retrieved, Some(&features));
    }

    #[test]
    fn cache_put_features_overwrites() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut cache = Cache::load(dir.path().join("dummy.json").as_path()).expect("empty cache");
        let id = TrackId::new("test-id");
        let features1 = crate::domain::TrackFeatures {
            tempo: crate::domain::Bpm::new(120.0).expect("valid bpm"),
            key: crate::domain::PitchClass::C,
            mode: crate::domain::Mode::Major,
            energy: crate::domain::Normalized::try_new(0.5).expect("valid norm"),
            danceability: crate::domain::Normalized::try_new(0.5).expect("valid norm"),
            valence: crate::domain::Normalized::try_new(0.5).expect("valid norm"),
            loudness: -5.5,
            acousticness: crate::domain::Normalized::try_new(0.2).expect("valid norm"),
            instrumentalness: crate::domain::Normalized::try_new(0.0).expect("valid norm"),
            liveness: crate::domain::Normalized::try_new(0.1).expect("valid norm"),
            speechiness: crate::domain::Normalized::try_new(0.05).expect("valid norm"),
        };
        let features2 = crate::domain::TrackFeatures {
            tempo: crate::domain::Bpm::new(140.0).expect("valid bpm"),
            ..features1
        };

        cache.put_features(id.clone(), features1);
        cache.put_features(id.clone(), features2.clone());

        let retrieved = cache.get_features(&id);
        assert_eq!(retrieved.map(|f| f.tempo.get()), Some(140.0));
    }

    #[test]
    fn cache_get_features_missing_returns_none() {
        let dir = tempfile::tempdir().expect("temp dir");
        let cache = Cache::load(dir.path().join("dummy.json").as_path()).expect("empty cache");
        let id = TrackId::new("nonexistent");
        assert_eq!(cache.get_features(&id), None);
    }

    // ========== Cache::save_atomic tests (Task 7) ==========

    #[test]
    fn cache_save_and_load_round_trip() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("cache.json");

        let mut cache = Cache::load(&path).expect("empty cache");
        let q1 = TrackQuery::new("Song 1", "Artist 1");
        let id1 = TrackId::new("id-1");
        let q2 = TrackQuery::new("Song 2", "Artist 2");
        let id2 = TrackId::new("id-2");

        cache.put_resolution(q1.clone(), id1.clone());
        cache.put_resolution(q2.clone(), id2.clone());

        let features = crate::domain::TrackFeatures {
            tempo: crate::domain::Bpm::new(120.0).expect("valid bpm"),
            key: crate::domain::PitchClass::C,
            mode: crate::domain::Mode::Major,
            energy: crate::domain::Normalized::try_new(0.5).expect("valid norm"),
            danceability: crate::domain::Normalized::try_new(0.6).expect("valid norm"),
            valence: crate::domain::Normalized::try_new(0.7).expect("valid norm"),
            loudness: -5.5,
            acousticness: crate::domain::Normalized::try_new(0.2).expect("valid norm"),
            instrumentalness: crate::domain::Normalized::try_new(0.0).expect("valid norm"),
            liveness: crate::domain::Normalized::try_new(0.1).expect("valid norm"),
            speechiness: crate::domain::Normalized::try_new(0.05).expect("valid norm"),
        };

        cache.put_features(id1.clone(), features.clone());
        cache.put_features(id2.clone(), features.clone());

        cache.save_atomic().expect("save");

        let loaded = Cache::load(&path).expect("load");
        assert_eq!(loaded.resolution_count(), 2);
        assert_eq!(loaded.feature_count(), 2);
        assert_eq!(loaded.get_resolution(&q1).map(|i| i.get()), Some("id-1"));
        assert_eq!(loaded.get_resolution(&q2).map(|i| i.get()), Some("id-2"));
    }

    #[test]
    fn cache_save_atomic_determinism() {
        let dir1 = tempfile::tempdir().expect("temp dir 1");
        let path1 = dir1.path().join("cache.json");
        let dir2 = tempfile::tempdir().expect("temp dir 2");
        let path2 = dir2.path().join("cache.json");

        // Create and save two identical caches.
        for path in [&path1, &path2] {
            let mut cache = Cache::load(path).expect("empty cache");

            let q1 = TrackQuery::new("Song A", "Artist A");
            let id1 = TrackId::new("id-a");
            let q2 = TrackQuery::new("Song B", "Artist B");
            let id2 = TrackId::new("id-b");

            cache.put_resolution(q1, id1.clone());
            cache.put_resolution(q2, id2.clone());

            let features = crate::domain::TrackFeatures {
                tempo: crate::domain::Bpm::new(125.0).expect("valid bpm"),
                key: crate::domain::PitchClass::new(2).expect("valid pitch"),
                mode: crate::domain::Mode::Minor,
                energy: crate::domain::Normalized::try_new(0.7).expect("valid norm"),
                danceability: crate::domain::Normalized::try_new(0.8).expect("valid norm"),
                valence: crate::domain::Normalized::try_new(0.3).expect("valid norm"),
                loudness: -3.2,
                acousticness: crate::domain::Normalized::try_new(0.1).expect("valid norm"),
                instrumentalness: crate::domain::Normalized::try_new(0.5).expect("valid norm"),
                liveness: crate::domain::Normalized::try_new(0.2).expect("valid norm"),
                speechiness: crate::domain::Normalized::try_new(0.15).expect("valid norm"),
            };

            cache.put_features(id1, features.clone());
            cache.put_features(id2, features);

            cache.save_atomic().expect("save");
        }

        // Both files should be byte-identical.
        let bytes1 = std::fs::read(&path1).expect("read 1");
        let bytes2 = std::fs::read(&path2).expect("read 2");

        assert_eq!(
            bytes1, bytes2,
            "two saves with identical data should produce byte-identical output"
        );
    }

    #[test]
    fn cache_save_atomic_preserves_existing_on_tmp_failure() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("cache.json");

        // Write a sentinel cache.
        let sentinel = r#"{"version":1,"resolutions":[],"features":{}}"#;
        std::fs::write(&path, sentinel).expect("write sentinel");

        // Load, modify, and try to save to a read-only parent (to fail temp write).
        let mut cache = Cache::load(&path).expect("load sentinel");
        let q = TrackQuery::new("New Song", "New Artist");
        let id = TrackId::new("new-id");
        cache.put_resolution(q, id);

        // Make parent read-only to force temp file creation to fail.
        let mut perms = std::fs::metadata(dir.path())
            .expect("stat dir")
            .permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(dir.path(), perms).expect("chmod");

        // Try to save — should fail.
        let result = cache.save_atomic();
        assert!(result.is_err(), "save should fail with read-only parent");

        // Restore read permission so we can verify.
        // Note: in tests, we bypass the Unix file permissions lint since we're just
        // cleaning up a temporary directory.
        #[allow(clippy::permissions_set_readonly_false)]
        {
            let mut perms = std::fs::metadata(dir.path())
                .expect("stat dir")
                .permissions();
            perms.set_readonly(false);
            std::fs::set_permissions(dir.path(), perms).expect("chmod");
        }

        // Existing file should be untouched.
        let content = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            content, sentinel,
            "original file should be unchanged after failed save"
        );
    }
}
