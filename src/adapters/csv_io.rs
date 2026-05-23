//! CSV input/output adapters for track queries, resolved tracks, and unresolved sidecars.

use std::path::Path;

use crate::domain::{Mode, Track, TrackQuery};
use crate::errors::InputError;

/// Read a CSV input file into track queries.
///
/// The input file must have at least `title` and `artist` columns (case-insensitive).
/// Extra columns are ignored. If a `reason` column is present, it is tolerated (for AC4.5 re-feed compatibility).
/// Empty rows (all cells blank after trim) are skipped.
///
/// Returns `InputError::NotFound` if the path does not exist.
/// Returns `InputError::MissingColumn` if required columns are missing.
/// Returns `InputError::NoRows` if the file has a valid header but no data rows.
/// Returns `InputError::Csv` for parse errors or empty file (no header).
pub fn read_input(path: &Path) -> Result<Vec<TrackQuery>, InputError> {
    if !path.exists() {
        return Err(InputError::NotFound {
            path: path.to_path_buf(),
        });
    }

    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|e| match e.kind() {
            csv::ErrorKind::Io(io_err) => InputError::Io {
                path: path.to_path_buf(),
                source: std::io::Error::new(io_err.kind(), e.to_string()),
            },
            _ => InputError::Csv {
                line: 0,
                message: e.to_string(),
                source: e,
            },
        })?;

    let headers = {
        let headers_result = rdr.headers();
        match headers_result {
            Ok(h) => h.clone(),
            Err(e) => {
                return Err(InputError::Csv {
                    line: 1,
                    message: e.to_string(),
                    source: e,
                })
            }
        }
    };

    let title_idx = find_column(&headers, "title");
    let artist_idx = find_column(&headers, "artist");

    let missing: Vec<String> = [("title", title_idx), ("artist", artist_idx)]
        .iter()
        .filter(|(_, idx)| idx.is_none())
        .map(|(name, _)| (*name).to_string())
        .collect();

    let (title_idx, artist_idx) = match (title_idx, artist_idx) {
        (Some(t), Some(a)) => (t, a),
        _ => {
            let header_src = headers.iter().collect::<Vec<_>>().join(",");
            let span = (0usize, header_src.len()).into();
            return Err(InputError::MissingColumn {
                missing,
                header_src,
                span,
            });
        }
    };

    let mut queries = Vec::new();
    for (i, row) in rdr.records().enumerate() {
        let row = row.map_err(|e| InputError::Csv {
            line: (i as u64) + 2,
            message: e.to_string(),
            source: e,
        })?;
        let title = row.get(title_idx).unwrap_or("").to_string();
        let artist = row.get(artist_idx).unwrap_or("").to_string();
        let q = TrackQuery::new(title, artist);
        if q.title.is_empty() && q.artist.is_empty() {
            continue;
        }
        queries.push(q);
    }

    if queries.is_empty() {
        return Err(InputError::NoRows {
            path: path.to_path_buf(),
        });
    }

    Ok(queries)
}

/// Write the reordered playlist + features to a CSV file.
///
/// Columns (in order):
///   position, title, artist, tempo, key, mode, energy,
///   danceability, valence, loudness, isrc
///
/// AC1.6: if the parent directory of `path` doesn't exist, returns
/// `InputError::MissingParentDir`. We do NOT create directories.
///
/// Fixed-decimal formatting ensures byte-deterministic output across platforms:
/// - tempo: 2 decimal places
/// - energy, danceability, valence: 4 decimal places
/// - loudness: 2 decimal places
pub fn write_output(path: &Path, ordering: &[usize], tracks: &[Track]) -> Result<(), InputError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(InputError::MissingParentDir {
                parent: parent.to_path_buf(),
            });
        }
    }

    let mut wtr = csv::WriterBuilder::new()
        .from_path(path)
        .map_err(|e| InputError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(e.to_string()),
        })?;

    wtr.write_record([
        "position",
        "title",
        "artist",
        "tempo",
        "key",
        "mode",
        "energy",
        "danceability",
        "valence",
        "loudness",
        "isrc",
    ])
    .map_err(|e| InputError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::other(e.to_string()),
    })?;

    for (pos, &idx) in ordering.iter().enumerate() {
        let t = &tracks[idx];
        let f = &t.features;
        wtr.write_record([
            (pos + 1).to_string(),
            t.query.title.clone(),
            t.query.artist.clone(),
            format!("{:.2}", f.tempo.get()),
            f.key.get().to_string(),
            match f.mode {
                Mode::Major => "major".to_string(),
                Mode::Minor => "minor".to_string(),
            },
            format!("{:.4}", f.energy.get()),
            format!("{:.4}", f.danceability.get()),
            format!("{:.4}", f.valence.get()),
            format!("{:.2}", f.loudness),
            t.id.get().to_string(),
        ])
        .map_err(|e| InputError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(e.to_string()),
        })?;
    }

    wtr.flush().map_err(|e| InputError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    Ok(())
}

/// A track query that failed to resolve through the adapter chain.
/// Used as the row shape of `unresolved.csv` and compatible with `read_input`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    /// The unresolved track query.
    pub query: TrackQuery,
    /// Human-readable explanation; e.g., "no MusicBrainz match",
    /// "MBID lookup returned no ISRCs", "ReccoBeats throttled".
    pub reason: String,
}

/// Write the unresolved-tracks sidecar to a CSV file.
///
/// Columns: title, artist, reason.
/// Compatible with `read_input` (the `reason` column is ignored on re-feed).
pub fn write_unresolved(path: &Path, unresolved: &[Unresolved]) -> Result<(), InputError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(InputError::MissingParentDir {
                parent: parent.to_path_buf(),
            });
        }
    }

    let mut wtr = csv::WriterBuilder::new()
        .from_path(path)
        .map_err(|e| InputError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(e.to_string()),
        })?;

    wtr.write_record(["title", "artist", "reason"])
        .map_err(|e| InputError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(e.to_string()),
        })?;

    for u in unresolved {
        wtr.write_record([
            u.query.title.clone(),
            u.query.artist.clone(),
            u.reason.clone(),
        ])
        .map_err(|e| InputError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(e.to_string()),
        })?;
    }

    wtr.flush().map_err(|e| InputError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    Ok(())
}

/// Helper: find a column by name (case-insensitive).
fn find_column(headers: &csv::StringRecord, name: &str) -> Option<usize> {
    headers.iter().position(|h| h.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== read_input tests ==========

    #[test]
    fn read_input_ac1_1_happy_path() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"title,artist\nGet Lucky,Daft Punk\n").expect("write temp");

        let result = read_input(temp.path()).expect("read");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "Get Lucky");
        assert_eq!(result[0].artist, "Daft Punk");
    }

    #[test]
    fn read_input_ac1_2_extra_columns_tolerated() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(
            temp.path(),
            b"title,artist,extra\nGet Lucky,Daft Punk,whatever\n",
        )
        .expect("write temp");

        let result = read_input(temp.path()).expect("read");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "Get Lucky");
        assert_eq!(result[0].artist, "Daft Punk");
    }

    #[test]
    fn read_input_ac1_3_not_found() {
        let path = std::path::Path::new("/nonexistent/path/to/file.csv");
        let result = read_input(path);
        assert!(matches!(result, Err(InputError::NotFound { .. })));
    }

    #[test]
    fn read_input_ac1_4_missing_artist() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"title,foo\nx,y\n").expect("write temp");

        let result = read_input(temp.path());
        match result {
            Err(InputError::MissingColumn { missing, .. }) => {
                assert!(missing.contains(&"artist".to_string()));
            }
            other => panic!("expected MissingColumn, got {:?}", other),
        }
    }

    #[test]
    fn read_input_ac1_4_missing_both() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"title2,artist2\nx,y\n").expect("write temp");

        let result = read_input(temp.path());
        match result {
            Err(InputError::MissingColumn { missing, .. }) => {
                assert!(missing.contains(&"title".to_string()));
                assert!(missing.contains(&"artist".to_string()));
            }
            other => panic!("expected MissingColumn, got {:?}", other),
        }
    }

    #[test]
    fn read_input_ac1_5_header_only() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"title,artist\n").expect("write temp");

        let result = read_input(temp.path());
        assert!(matches!(result, Err(InputError::NoRows { .. })));
    }

    #[test]
    fn read_input_ac1_5_empty_file() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"").expect("write temp");

        let result = read_input(temp.path());
        // Empty file is parsed as header with zero columns, triggering MissingColumn
        assert!(matches!(result, Err(InputError::MissingColumn { .. })));
    }

    #[test]
    fn read_input_ac4_5_sidecar_compat_reason_column() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"title,artist,reason\nx,y,no match\n").expect("write temp");

        let result = read_input(temp.path()).expect("read");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "x");
        assert_eq!(result[0].artist, "y");
    }

    #[test]
    fn read_input_trim_cells() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"title,artist\n  A , B \n").expect("write temp");

        let result = read_input(temp.path()).expect("read");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "A");
        assert_eq!(result[0].artist, "B");
    }

    #[test]
    fn read_input_case_insensitive_header() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"Title,ARTIST\nx,y\n").expect("write temp");

        let result = read_input(temp.path()).expect("read");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "x");
        assert_eq!(result[0].artist, "y");
    }

    #[test]
    fn read_input_empty_row_skipped() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(temp.path(), b"title,artist\nx,y\n,\nz,w\n").expect("write temp");

        let result = read_input(temp.path()).expect("read");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].title, "x");
        assert_eq!(result[0].artist, "y");
        assert_eq!(result[1].title, "z");
        assert_eq!(result[1].artist, "w");
    }

    // ========== write_output tests ==========

    #[test]
    fn write_output_happy_path() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");

        let tracks = vec![
            Track {
                query: TrackQuery::new("Track 1", "Artist 1"),
                id: crate::domain::TrackId::new("id1"),
                features: crate::domain::TrackFeatures {
                    tempo: crate::domain::Bpm::new(120.0).expect("valid bpm"),
                    key: crate::domain::PitchClass::C,
                    mode: Mode::Major,
                    energy: crate::domain::Normalized::try_new(0.5).expect("valid norm"),
                    danceability: crate::domain::Normalized::try_new(0.6).expect("valid norm"),
                    valence: crate::domain::Normalized::try_new(0.7).expect("valid norm"),
                    loudness: -5.5,
                    acousticness: crate::domain::Normalized::try_new(0.2).expect("valid norm"),
                    instrumentalness: crate::domain::Normalized::try_new(0.0).expect("valid norm"),
                    liveness: crate::domain::Normalized::try_new(0.1).expect("valid norm"),
                    speechiness: crate::domain::Normalized::try_new(0.05).expect("valid norm"),
                },
            },
            Track {
                query: TrackQuery::new("Track 2", "Artist 2"),
                id: crate::domain::TrackId::new("id2"),
                features: crate::domain::TrackFeatures {
                    tempo: crate::domain::Bpm::new(130.0).expect("valid bpm"),
                    key: crate::domain::PitchClass::new(2).expect("valid pitch"),
                    mode: Mode::Minor,
                    energy: crate::domain::Normalized::try_new(0.8).expect("valid norm"),
                    danceability: crate::domain::Normalized::try_new(0.75).expect("valid norm"),
                    valence: crate::domain::Normalized::try_new(0.4).expect("valid norm"),
                    loudness: -3.2,
                    acousticness: crate::domain::Normalized::try_new(0.1).expect("valid norm"),
                    instrumentalness: crate::domain::Normalized::try_new(0.5).expect("valid norm"),
                    liveness: crate::domain::Normalized::try_new(0.3).expect("valid norm"),
                    speechiness: crate::domain::Normalized::try_new(0.02).expect("valid norm"),
                },
            },
            Track {
                query: TrackQuery::new("Track 3", "Artist 3"),
                id: crate::domain::TrackId::new("id3"),
                features: crate::domain::TrackFeatures {
                    tempo: crate::domain::Bpm::new(100.0).expect("valid bpm"),
                    key: crate::domain::PitchClass::new(4).expect("valid pitch"),
                    mode: Mode::Major,
                    energy: crate::domain::Normalized::try_new(0.3).expect("valid norm"),
                    danceability: crate::domain::Normalized::try_new(0.4).expect("valid norm"),
                    valence: crate::domain::Normalized::try_new(0.9).expect("valid norm"),
                    loudness: -8.0,
                    acousticness: crate::domain::Normalized::try_new(0.9).expect("valid norm"),
                    instrumentalness: crate::domain::Normalized::try_new(0.0).expect("valid norm"),
                    liveness: crate::domain::Normalized::try_new(0.2).expect("valid norm"),
                    speechiness: crate::domain::Normalized::try_new(0.01).expect("valid norm"),
                },
            },
        ];

        let ordering = vec![0, 1, 2];
        write_output(temp.path(), &ordering, &tracks).expect("write");

        let content = std::fs::read_to_string(temp.path()).expect("read back");
        let lines: Vec<&str> = content.lines().collect();

        // Check header
        assert_eq!(
            lines[0],
            "position,title,artist,tempo,key,mode,energy,danceability,valence,loudness,isrc"
        );

        // Check we have 3 data rows + 1 header = 4 lines
        assert_eq!(lines.len(), 4);

        // Check each row has 11 comma-separated fields
        for (idx, line) in lines.iter().enumerate().skip(1) {
            let field_count = line.split(',').count();
            assert_eq!(field_count, 11, "row {} should have 11 fields", idx);
        }
    }

    #[test]
    fn write_output_ac1_6_missing_parent_dir() {
        let path = std::path::Path::new("/nonexistent/dir/out.csv");
        let tracks = vec![];
        let ordering = vec![];

        let result = write_output(path, &ordering, &tracks);
        assert!(matches!(result, Err(InputError::MissingParentDir { .. })));
    }

    #[test]
    fn write_output_determinism() {
        let temp1 = tempfile::NamedTempFile::new().expect("temp file 1");
        let temp2 = tempfile::NamedTempFile::new().expect("temp file 2");

        let tracks = vec![Track {
            query: TrackQuery::new("Song", "Band"),
            id: crate::domain::TrackId::new("xyz"),
            features: crate::domain::TrackFeatures {
                tempo: crate::domain::Bpm::new(125.0).expect("valid bpm"),
                key: crate::domain::PitchClass::new(7).expect("valid pitch"),
                mode: Mode::Major,
                energy: crate::domain::Normalized::try_new(0.5).expect("valid norm"),
                danceability: crate::domain::Normalized::try_new(0.6).expect("valid norm"),
                valence: crate::domain::Normalized::try_new(0.7).expect("valid norm"),
                loudness: -5.5,
                acousticness: crate::domain::Normalized::try_new(0.2).expect("valid norm"),
                instrumentalness: crate::domain::Normalized::try_new(0.0).expect("valid norm"),
                liveness: crate::domain::Normalized::try_new(0.1).expect("valid norm"),
                speechiness: crate::domain::Normalized::try_new(0.05).expect("valid norm"),
            },
        }];

        let ordering = vec![0];

        write_output(temp1.path(), &ordering, &tracks).expect("write 1");
        write_output(temp2.path(), &ordering, &tracks).expect("write 2");

        let bytes1 = std::fs::read(temp1.path()).expect("read 1");
        let bytes2 = std::fs::read(temp2.path()).expect("read 2");

        assert_eq!(
            bytes1, bytes2,
            "two writes should produce byte-identical output"
        );
    }

    // ========== write_unresolved tests ==========

    #[test]
    fn write_unresolved_happy_path() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");

        let unresolved = vec![
            Unresolved {
                query: TrackQuery::new("Lost Song", "Unknown Artist"),
                reason: "no MusicBrainz match".to_string(),
            },
            Unresolved {
                query: TrackQuery::new("Obscure Track", "Indie Band"),
                reason: "MBID lookup returned no ISRCs".to_string(),
            },
        ];

        write_unresolved(temp.path(), &unresolved).expect("write");

        let content = std::fs::read_to_string(temp.path()).expect("read back");
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(lines[0], "title,artist,reason");
        assert_eq!(lines.len(), 3); // header + 2 data rows
    }

    #[test]
    fn write_unresolved_ac4_5_round_trip() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");

        let original_unresolved = vec![
            Unresolved {
                query: TrackQuery::new("Song A", "Artist A"),
                reason: "no match".to_string(),
            },
            Unresolved {
                query: TrackQuery::new("Song B", "Artist B"),
                reason: "throttled".to_string(),
            },
        ];

        write_unresolved(temp.path(), &original_unresolved).expect("write");

        let queries = read_input(temp.path()).expect("read_input");

        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0].title, original_unresolved[0].query.title);
        assert_eq!(queries[0].artist, original_unresolved[0].query.artist);
        assert_eq!(queries[1].title, original_unresolved[1].query.title);
        assert_eq!(queries[1].artist, original_unresolved[1].query.artist);
    }

    #[test]
    fn write_unresolved_empty_writes_header_only() {
        let temp = tempfile::NamedTempFile::new().expect("temp file");

        write_unresolved(temp.path(), &[]).expect("write");

        let result = read_input(temp.path());
        assert!(matches!(result, Err(InputError::NoRows { .. })));
    }
}
