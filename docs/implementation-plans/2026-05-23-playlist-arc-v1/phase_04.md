# Playlist Arc v1 — Phase 4: File IO — CSV and JSON Cache

**Goal:** Read input CSVs, write output and unresolved CSVs, and persist a versioned JSON cache with atomic-write semantics. All IO lives in `src/adapters/`; nothing in `algo/` or `domain/` touches the disk.

**Architecture:** Two adapter modules — `csv_io.rs` for CSV read/write and `cache.rs` for the JSON sidecar. Both surface structured `thiserror` + `miette::Diagnostic` errors that include source paths and line numbers. The cache is the determinism anchor (per design: "Two runs are byte-identical only when input CSV, cache state, *and* seed match.").

**Tech Stack:** Rust, `csv` (reader/writer), `serde_json` (cache file), `miette` (diagnostic spans), `tempfile`-free atomic rename (write to `<path>.tmp`, then `std::fs::rename`).

**Scope:** Phase 4 of 7 from `<project-root>/docs/design-plans/2026-05-23-playlist-arc-v1.md`.

**Codebase verified:** 2026-05-23 — Phase 2 domain types will exist (`TrackQuery`, `TrackId`, `Track`, `TrackFeatures`). `src/adapters/mod.rs` exists as a stub. `src/errors.rs` exists with a module-level doc comment but no types yet.

**Project guidance:** `<project-root>/implementation-plan-guidance.md`. Key rules for this phase: errors must include source paths + line numbers (per the "miette source spans" rule); no `unwrap()` outside tests/`main`; new `pub` items need doc comments.

---

## Acceptance Criteria Coverage

This phase implements and tests:

### playlist-arc-v1.AC1: Input CSV is read, reordered, augmented with features, and written
- **playlist-arc-v1.AC1.1 Success:** Input CSV with `title,artist` header and N rows produces output CSV with the same N tracks reordered, with feature columns (`position, tempo, key, mode, energy, danceability, valence, loudness, isrc`) appended.
- **playlist-arc-v1.AC1.2 Success:** Extra columns in input CSV are tolerated (ignored, not preserved in output).
- **playlist-arc-v1.AC1.3 Failure:** Input path does not exist → exit 3 with `InputError::NotFound`, miette message identifying the path.
- **playlist-arc-v1.AC1.4 Failure:** Missing `title` or `artist` header → exit 3 with `InputError::Csv` pointing at the header row.
- **playlist-arc-v1.AC1.5 Failure:** Header-only / zero-row input → exit 3 with a clear "no tracks" message; no silent empty output.
- **playlist-arc-v1.AC1.6 Edge:** Output path's parent directory does not exist → exit 3 with a clear message; the binary does not create directories.

(Exit-code behavior is verified end-to-end in Phase 7; this phase verifies the *error values* are constructed correctly. AC1.1 column shape is verified here as a unit test on the writer; the full round-trip is Phase 7.)

### playlist-arc-v1.AC4: Unresolved tracks are logged and written to a sidecar CSV
- **playlist-arc-v1.AC4.1 Success:** Unresolvable queries are omitted from the optimizer input and written to `unresolved.csv` with `title,artist,reason` columns.
- **playlist-arc-v1.AC4.5 Edge:** Sidecar rows are re-feedable (compatible with `read_input`).

(AC4.2, AC4.3, AC4.4 are orchestration concerns covered in Phases 5/7.)

### playlist-arc-v1.AC7: Cache hits zero network on warm runs
- **playlist-arc-v1.AC7.2 Success:** Cache is written atomically (temp + rename); a mid-run SIGKILL leaves the previous cache intact.
- **playlist-arc-v1.AC7.3 Failure:** Cache `version` mismatch → exit 4 with `CacheError::VersionMismatch` and a "delete or upgrade" hint.
- **playlist-arc-v1.AC7.4 Edge:** Corrupt JSON → exit 4 with `CacheError::Corrupt` and the underlying serde error attached.

(AC7.1 is verified by the Phase 7 `tests/zero_network.rs` integration test.)

---

## Task Overview

```
SUBCOMPONENT_A: Structured error types (task 1)
SUBCOMPONENT_B: CSV reader + writers (tasks 2-4)
SUBCOMPONENT_C: Cache with atomic save (tasks 5-7)
SUBCOMPONENT_D: Module re-exports and verification (task 8)
```

---

<!-- START_SUBCOMPONENT_A (task 1) -->

<!-- START_TASK_1 -->
### Task 1: Define InputError and CacheError in errors.rs

**Files:**
- Modify: `<project-root>/src/errors.rs`

**Implementation:**

The design specifies miette source spans for these errors. Use `#[source_code]` and `#[label]` attributes from `miette::Diagnostic` so the CLI can render readable error reports with the offending CSV/JSON pointed at.

```rust
//! Structured error types. Each derives `thiserror::Error` and
//! `miette::Diagnostic` so the CLI can render source spans + help text.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum InputError {
    #[error("input file not found: {path}")]
    #[diagnostic(
        code(order_playlist::input::not_found),
        help("verify the path exists and is readable")
    )]
    NotFound { path: PathBuf },

    #[error("output parent directory does not exist: {parent}")]
    #[diagnostic(
        code(order_playlist::input::missing_parent),
        help("create the directory before running, or choose a path whose parent exists")
    )]
    MissingParentDir { parent: PathBuf },

    #[error("missing required column(s): {missing:?}")]
    #[diagnostic(
        code(order_playlist::input::missing_column),
        help("input CSV must have at minimum 'title' and 'artist' columns")
    )]
    MissingColumn {
        missing: Vec<String>,
        #[source_code]
        header_src: String,
        #[label("header line")]
        span: miette::SourceSpan,
    },

    #[error("input contained zero data rows")]
    #[diagnostic(
        code(order_playlist::input::no_rows),
        help("the file has a valid header but no tracks; add at least one row")
    )]
    NoRows { path: PathBuf },

    #[error("CSV parse error at line {line}: {message}")]
    #[diagnostic(code(order_playlist::input::csv_parse))]
    Csv {
        line: u64,
        message: String,
        #[source]
        source: csv::Error,
    },

    #[error("IO error reading {path}")]
    #[diagnostic(code(order_playlist::input::io))]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum CacheError {
    #[error("cache version mismatch: file has version {found}, expected {expected}")]
    #[diagnostic(
        code(order_playlist::cache::version_mismatch),
        help("delete the cache file or upgrade `order_playlist`; cache schema changed between versions")
    )]
    VersionMismatch { found: u32, expected: u32 },

    #[error("cache file is corrupt: {message}")]
    #[diagnostic(
        code(order_playlist::cache::corrupt),
        help("delete the cache file and rerun; resolved features will be re-fetched")
    )]
    Corrupt {
        message: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("IO error on cache file {path}")]
    #[diagnostic(code(order_playlist::cache::io))]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
```

Note the `csv::Error` and `serde_json::Error` types are wrapped as `#[source]` — they are NOT marked `#[from]` because the wrapping carries extra context (line number, path).

**Testing:**

Unit tests on construction:
- `InputError::NotFound { path: "/missing".into() }` formats with the path visible.
- `InputError::MissingColumn { missing: vec!["artist".into()], header_src: "title,foo".into(), span: (0, 11).into() }` constructs without panic.
- `CacheError::VersionMismatch { found: 1, expected: 2 }` formats with both numbers visible.

The error-rendering tests just confirm construction and `Display` output — they don't yet exercise miette's fancy renderer (that's verified end-to-end in Phase 7).

**Verification:**

Run: `cd <project-root> && cargo build && cargo test --lib errors`
Expected: green.

**Commit:** `Phase 4: InputError + CacheError with miette diagnostics`
<!-- END_TASK_1 -->

<!-- END_SUBCOMPONENT_A -->

<!-- START_SUBCOMPONENT_B (tasks 2-4) -->

<!-- START_TASK_2 -->
### Task 2: Implement read_input(path) -> Result<Vec<TrackQuery>, InputError>

**Files:**
- Create: `<project-root>/src/adapters/csv_io.rs`

**Implementation:**

`read_input` must:
1. Return `InputError::NotFound` if the path doesn't exist (check `path.exists()` before opening so the error is distinguishable from a generic permission error).
2. Open the file and read its header. Trim each column name (`csv::ReaderBuilder::new().trim(csv::Trim::All)`).
3. Validate that `title` and `artist` (case-insensitive lookup) are present. If missing, return `InputError::MissingColumn` with a miette source-span pointing at the header line. The header line is line 1; the source span covers the column-name area.
4. Read every data row. For each row, extract `title` and `artist` by case-insensitive column name; ignore any other columns (AC1.2: extra columns tolerated).
5. If zero data rows, return `InputError::NoRows`. Empty input file (no header at all) returns `InputError::Csv` with a clear message.
6. Trim leading/trailing whitespace from each cell (`TrackQuery::new` already does this — wrap with `TrackQuery::new(title, artist)`).
7. **Sidecar compatibility (AC4.5).** If the file *also* contains a `reason` column, ignore it. This makes `unresolved.csv` re-feedable as input.

```rust
use std::path::Path;
use crate::domain::TrackQuery;
use crate::errors::InputError;

pub fn read_input(path: &Path) -> Result<Vec<TrackQuery>, InputError> {
    if !path.exists() {
        return Err(InputError::NotFound { path: path.to_path_buf() });
    }

    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|e| InputError::Io { path: path.to_path_buf(), source: io_from_csv_err(&e) })?;

    let headers = rdr.headers()
        .map_err(|e| InputError::Csv { line: 1, message: e.to_string(), source: e.clone() })?
        .clone();

    let title_idx = find_column(&headers, "title");
    let artist_idx = find_column(&headers, "artist");

    let missing: Vec<String> = [
        ("title", title_idx), ("artist", artist_idx),
    ].iter().filter(|(_, idx)| idx.is_none())
     .map(|(name, _)| (*name).to_string())
     .collect();

    // Bubble the validation as one unit: if either column is missing,
    // return the error; otherwise destructure both indices in a single
    // pattern with no `unwrap()` (project rule: no `unwrap()` outside
    // tests/main).
    let (title_idx, artist_idx) = match (title_idx, artist_idx) {
        (Some(t), Some(a)) => (t, a),
        _ => {
            let header_src = headers.iter().collect::<Vec<_>>().join(",");
            let span = (0usize, header_src.len()).into();
            return Err(InputError::MissingColumn { missing, header_src, span });
        }
    };

    let mut queries = Vec::new();
    for (i, row) in rdr.records().enumerate() {
        let row = row.map_err(|e| InputError::Csv {
            // CSV line numbers are 1-based and skip the header.
            line: (i as u64) + 2,
            message: e.to_string(),
            source: e,
        })?;
        let title = row.get(title_idx).unwrap_or("").to_string();
        let artist = row.get(artist_idx).unwrap_or("").to_string();
        // TrackQuery::new trims; skip rows that are empty after trim.
        let q = TrackQuery::new(title, artist);
        if q.title.is_empty() && q.artist.is_empty() {
            continue;
        }
        queries.push(q);
    }

    if queries.is_empty() {
        return Err(InputError::NoRows { path: path.to_path_buf() });
    }

    Ok(queries)
}

fn find_column(headers: &csv::StringRecord, name: &str) -> Option<usize> {
    headers.iter().position(|h| h.eq_ignore_ascii_case(name))
}
```

The `io_from_csv_err` helper: `csv::Error` wraps a `std::io::Error` when the underlying failure is IO. If not, synthesize one. Cleaner: pattern-match `e.kind()` if it's `csv::ErrorKind::Io`, otherwise emit `InputError::Csv` instead. Replace the `map_err` above with the cleaner version:

```rust
.from_path(path)
.map_err(|e| match e.kind() {
    csv::ErrorKind::Io(io_err) => InputError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(io_err.kind(), e.to_string()),
    },
    _ => InputError::Csv { line: 0, message: e.to_string(), source: e },
})?;
```

The code above has zero `unwrap()` calls (project rule: no `unwrap()` outside tests/`main`). `row.get(title_idx).unwrap_or("")` uses `unwrap_or` (a total function with a default) and is allowed. The tuple-destructure pattern in the validation block replaces what would have been two `.unwrap()` calls on the already-validated `Option<usize>` indices.

**Testing (in `#[cfg(test)] mod tests` of the same file):**

For each case below, create a temp file with `tempfile::NamedTempFile` (add `tempfile = "=3.13.0"` to `[dev-dependencies]` in Cargo.toml — Phase 1 didn't list this; document the addition in the commit message).

| Case | Input | Expected |
|------|-------|----------|
| AC1.1 happy path | `title,artist\nGet Lucky,Daft Punk\n` | Ok(vec with one TrackQuery) |
| AC1.2 extra columns | `title,artist,extra\nGet Lucky,Daft Punk,whatever\n` | Ok — extra column ignored |
| AC1.3 not found | path that doesn't exist | Err(InputError::NotFound) |
| AC1.4 missing artist | `title,foo\nx,y\n` | Err(InputError::MissingColumn { missing: ["artist"], .. }) |
| AC1.4 missing both | `title2,artist2\nx,y\n` | Err(InputError::MissingColumn { missing: ["title", "artist"], .. }) |
| AC1.5 header-only | `title,artist\n` | Err(InputError::NoRows) |
| AC1.5 empty file | `` | Err(InputError::Csv) (zero-byte file fails to parse header) |
| AC4.5 sidecar compat | `title,artist,reason\nx,y,no match\n` | Ok(vec with one query) |
| Trim cells | `title,artist\n  A , B \n` | Ok with TrackQuery { title: "A", artist: "B" } |
| Case-insensitive header | `Title,ARTIST\nx,y\n` | Ok(vec with one query) |
| Empty row skipped | `title,artist\nx,y\n,\nz,w\n` | Ok(vec with 2 queries — middle empty row dropped) |

**Verification:**

Run: `cd <project-root> && cargo test --lib adapters::csv_io::tests::read_input`
Expected: all cases pass.

**Commit:** `Phase 4: csv_io::read_input with miette-decorated errors`
<!-- END_TASK_2 -->

<!-- START_TASK_3 -->
### Task 3: Implement write_output(path, ordering, &[Track])

**Files:**
- Modify: `<project-root>/src/adapters/csv_io.rs`

**Implementation:**

`write_output` writes the reordered tracks with feature columns appended. The column order is fixed by AC1.1: `position, title, artist, tempo, key, mode, energy, danceability, valence, loudness, isrc`. (The AC lists the *feature* columns starting at `position`; `title`/`artist` come from the original `TrackQuery`. Document the full column list in the function docstring.)

```rust
use std::path::Path;
use crate::domain::Track;
use crate::errors::InputError;

/// Write the reordered playlist + features to `path`.
///
/// Columns (in order):
///   position, title, artist, tempo, key, mode, energy,
///   danceability, valence, loudness, isrc
///
/// AC1.6: if the parent directory of `path` doesn't exist, returns
/// `InputError::MissingParentDir`. We do NOT create directories.
pub fn write_output(path: &Path, ordering: &[usize], tracks: &[Track]) -> Result<(), InputError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(InputError::MissingParentDir { parent: parent.to_path_buf() });
        }
    }

    let mut wtr = csv::WriterBuilder::new()
        .from_path(path)
        .map_err(|e| InputError::Io {
            path: path.to_path_buf(),
            source: std::io::Error::other(e.to_string()),
        })?;

    wtr.write_record([
        "position", "title", "artist", "tempo", "key", "mode",
        "energy", "danceability", "valence", "loudness", "isrc",
    ]).map_err(|e| InputError::Io {
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
            match f.mode { crate::domain::Mode::Major => "major", crate::domain::Mode::Minor => "minor" }.to_string(),
            format!("{:.4}", f.energy.get()),
            format!("{:.4}", f.danceability.get()),
            format!("{:.4}", f.valence.get()),
            format!("{:.2}", f.loudness),
            t.id.get().to_string(),
        ]).map_err(|e| InputError::Io {
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
```

The fixed-decimal formatting (`{:.2}`, `{:.4}`) is chosen for byte-identical determinism (AC5.1) — float-to-string with `Display` produces "the shortest round-trippable form" which can vary across platforms.

**Testing:**

- Happy path: write 3 tracks to a temp file, read back as text, assert the header line and that each row has 11 comma-separated fields.
- AC1.6: `write_output("/nonexistent/dir/out.csv", ...)` returns `InputError::MissingParentDir`.
- AC1.1 column order: snapshot-test the header line via `insta::assert_snapshot!`.
- Determinism: writing the same tracks twice produces byte-identical files (`std::fs::read_to_string` × 2, compare).

**Verification:**

Run: `cd <project-root> && cargo test --lib adapters::csv_io::tests::write_output`
Expected: all cases pass; `insta` snapshot accepted (run `cargo insta review` if first time).

**Commit:** `Phase 4: csv_io::write_output with fixed-precision feature columns`
<!-- END_TASK_3 -->

<!-- START_TASK_4 -->
### Task 4: Implement write_unresolved(path, &[Unresolved])

**Files:**
- Modify: `<project-root>/src/adapters/csv_io.rs`

**Implementation:**

Define an `Unresolved` value type in this file (a small adapter-layer struct — not domain, because the algorithm never sees it):

```rust
/// A single track that failed to resolve through the adapter chain.
/// Used as the row shape of `unresolved.csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    pub query: crate::domain::TrackQuery,
    /// Human-readable explanation; e.g., "no MusicBrainz match",
    /// "MBID lookup returned no ISRCs", "ReccoBeats throttled".
    pub reason: String,
}

/// Write the unresolved-tracks sidecar to `path`.
///
/// Columns: title, artist, reason.
/// Compatible with `read_input` (the `reason` column is ignored on re-feed).
pub fn write_unresolved(path: &Path, unresolved: &[Unresolved]) -> Result<(), InputError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(InputError::MissingParentDir { parent: parent.to_path_buf() });
        }
    }

    let mut wtr = csv::WriterBuilder::new().from_path(path)
        .map_err(/* same Io mapping as above */)?;

    wtr.write_record(["title", "artist", "reason"])
        .map_err(/* same */)?;

    for u in unresolved {
        wtr.write_record([
            u.query.title.clone(),
            u.query.artist.clone(),
            u.reason.clone(),
        ]).map_err(/* same */)?;
    }

    wtr.flush().map_err(|e| InputError::Io { path: path.to_path_buf(), source: e })?;
    Ok(())
}
```

**Testing:**

- Write two unresolved rows, read back as text, assert header + row content.
- AC4.5 round-trip: `write_unresolved` to a temp file, then call `read_input` on the same file, assert the returned `Vec<TrackQuery>` matches the original queries.
- Empty `unresolved` (no rows): writes only the header, file exists, `read_input` returns `InputError::NoRows`.

**Verification:**

Run: `cd <project-root> && cargo test --lib adapters::csv_io::tests::write_unresolved`
Expected: all cases pass.

**Commit:** `Phase 4: csv_io::write_unresolved + AC4.5 re-feed round-trip test`
<!-- END_TASK_4 -->

<!-- END_SUBCOMPONENT_B -->

<!-- START_SUBCOMPONENT_C (tasks 5-7) -->

<!-- START_TASK_5 -->
### Task 5: Implement CacheFile struct + Cache::load (with version + corrupt handling)

**Files:**
- Create: `<project-root>/src/adapters/cache.rs`

**Implementation:**

```rust
//! Versioned JSON sidecar that stores resolved IDs and audio features.
//!
//! File schema (incremented on any breaking change):
//!   {
//!     "version": 1,
//!     "resolutions": { "<TrackQuery as JSON object>": "<TrackId string>", ... },
//!     "features": { "<TrackId string>": <TrackFeatures>, ... }
//!   }
//!
//! Atomic-write contract: `Cache::save_atomic(path)` writes to
//! `path.with_extension("json.tmp")` first, then `std::fs::rename` into
//! place. A mid-run SIGKILL leaves the existing `path` untouched.

use std::collections::HashMap;
use std::path::Path;
use crate::domain::{TrackFeatures, TrackId, TrackQuery};
use crate::errors::CacheError;

/// Schema version. Bump on any breaking change to `resolutions` or `features`.
pub const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheFile {
    pub version: u32,
    /// `TrackQuery` is a struct, not a string, so HashMap<TrackQuery, _>
    /// can't be a JSON object key directly. We side-step by representing
    /// `resolutions` as a `Vec<(TrackQuery, TrackId)>` in the serialized
    /// form. The accessors below maintain a HashMap in memory.
    #[serde(rename = "resolutions")]
    pub resolutions: Vec<(TrackQuery, TrackId)>,
    /// `TrackId` is a newtype wrapping a String, which CAN be a JSON key.
    pub features: HashMap<TrackId, TrackFeatures>,
}

impl Default for CacheFile {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            resolutions: Vec::new(),
            features: HashMap::new(),
        }
    }
}

pub struct Cache {
    path: std::path::PathBuf,
    /// In-memory mirror of resolutions for O(1) lookup.
    resolutions: HashMap<TrackQuery, TrackId>,
    features: HashMap<TrackId, TrackFeatures>,
}

impl Cache {
    /// Load the cache from `path`. If the file doesn't exist, returns
    /// an empty cache (warm-cache path is identified by `path.exists()`
    /// in the caller; non-existence is not an error).
    pub fn load(path: &Path) -> Result<Self, CacheError> {
        if !path.exists() {
            return Ok(Self {
                path: path.to_path_buf(),
                resolutions: HashMap::new(),
                features: HashMap::new(),
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

        let resolutions: HashMap<TrackQuery, TrackId> = file.resolutions.into_iter().collect();

        Ok(Self {
            path: path.to_path_buf(),
            resolutions,
            features: file.features,
        })
    }
}
```

`TrackId` (a newtype wrapping `String`) needs `serde::Serialize` + `serde::Deserialize` on the inner string for `HashMap<TrackId, _>` to serialize cleanly. Phase 2 should have derived these — verify before using.

**Testing:**

- `Cache::load(nonexistent_path)` returns an empty `Cache` (no error).
- `Cache::load(empty_file)` returns `CacheError::Corrupt` (`{}` would also corrupt since it lacks `version`).
- `Cache::load(json_with_version_99)` returns `CacheError::VersionMismatch { found: 99, expected: 1 }`.
- Cache round-trip (serialize CacheFile → write to temp file → read back via `Cache::load`) preserves both maps.

**Verification:**

Run: `cd <project-root> && cargo test --lib adapters::cache::tests::load`
Expected: all cases pass.

**Commit:** `Phase 4: Cache::load with version check + corrupt diagnostics`
<!-- END_TASK_5 -->

<!-- START_TASK_6 -->
### Task 6: Implement Cache accessors (get/put for resolutions and features)

**Files:**
- Modify: `<project-root>/src/adapters/cache.rs`

**Implementation:**

```rust
impl Cache {
    pub fn get_resolution(&self, q: &TrackQuery) -> Option<&TrackId> {
        self.resolutions.get(q)
    }

    /// Insert or overwrite the resolution for `q`. Caller is responsible
    /// for calling `save_atomic` before the process exits — `put` is
    /// in-memory only.
    pub fn put_resolution(&mut self, q: TrackQuery, id: TrackId) {
        self.resolutions.insert(q, id);
    }

    pub fn get_features(&self, id: &TrackId) -> Option<&TrackFeatures> {
        self.features.get(id)
    }

    pub fn put_features(&mut self, id: TrackId, features: TrackFeatures) {
        self.features.insert(id, features);
    }

    pub fn resolution_count(&self) -> usize { self.resolutions.len() }
    pub fn feature_count(&self) -> usize { self.features.len() }
}
```

**Testing:**

- `put_resolution` then `get_resolution` returns the inserted value.
- `put_resolution` twice with the same key overwrites.
- `get_resolution` on missing key returns None.
- Mirror tests for features.

**Verification:**

Run: `cd <project-root> && cargo test --lib adapters::cache::tests::accessors`
Expected: green.

**Commit:** Bundled with Task 7.
<!-- END_TASK_6 -->

<!-- START_TASK_7 -->
### Task 7: Implement Cache::save_atomic and verify the SIGKILL-safe property

**Files:**
- Modify: `<project-root>/src/adapters/cache.rs`

**Implementation:**

```rust
impl Cache {
    /// Atomically persist the cache to disk:
    ///   1. Build a CacheFile from in-memory state.
    ///   2. Serialize to JSON bytes.
    ///   3. Write to `<path>.tmp`.
    ///   4. `fsync` the temp file.
    ///   5. `std::fs::rename(<path>.tmp, <path>)`.
    ///
    /// If step 3 or 4 fails, the existing file at `path` is untouched.
    /// If step 5 fails, the temp file is left behind (caller may want
    /// to clean up — we don't, because mid-flight crashes are the case
    /// this guards against).
    pub fn save_atomic(&self) -> Result<(), CacheError> {
        let file = CacheFile {
            version: CACHE_VERSION,
            resolutions: self.resolutions.iter()
                .map(|(q, id)| (q.clone(), id.clone()))
                .collect(),
            features: self.features.clone(),
        };

        let bytes = serde_json::to_vec_pretty(&file).map_err(|e| CacheError::Corrupt {
            message: format!("failed to serialize cache: {}", e),
            source: e,
        })?;

        let tmp_path = self.path.with_extension(
            self.path.extension()
                .map(|e| format!("{}.tmp", e.to_string_lossy()))
                .unwrap_or_else(|| "tmp".to_string())
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
```

**Determinism note.** `HashMap` iteration order is non-deterministic in Rust, which breaks byte-identical serialization. Use `BTreeMap` instead of `HashMap` for `resolutions` and `features` in `CacheFile` (and the in-memory `Cache` mirror) so the serialized JSON has stable key order. Update the field types accordingly; everything else (load/save/accessors) stays the same since `BTreeMap` shares the same `get`/`insert`/`iter` API.

**Testing:**

- Save then load: serialize a cache with 3 resolutions and 3 features, save to a temp path, load it back, assert maps match.
- **Atomicity test (AC7.2)**:
  ```rust
  #[test]
  fn save_atomic_preserves_existing_on_temp_failure() {
      let dir = tempfile::tempdir().unwrap();
      let path = dir.path().join("cache.json");
      // Write a sentinel file.
      std::fs::write(&path, b"{\"version\":1,\"resolutions\":[],\"features\":{}}").unwrap();
      // Simulate "tmp write fails": make the parent directory read-only.
      let mut perms = std::fs::metadata(dir.path()).unwrap().permissions();
      perms.set_readonly(true);
      std::fs::set_permissions(dir.path(), perms).unwrap();
      // Try to save — should fail.
      let mut cache = Cache::load(&path).unwrap();
      cache.put_resolution(/* something */);
      let result = cache.save_atomic();
      assert!(result.is_err());
      // Reset perms so we can read.
      let mut perms = std::fs::metadata(dir.path()).unwrap().permissions();
      perms.set_readonly(false);
      std::fs::set_permissions(dir.path(), perms).unwrap();
      // Existing file is unchanged.
      let contents = std::fs::read_to_string(&path).unwrap();
      assert!(contents.contains("\"version\":1"));
      assert!(!contents.contains("/* new resolution */"));
  }
  ```
  On macOS, read-only on a parent directory may not prevent overwrites of files inside it (depends on uid). If the test is flaky, alternative: write to a path whose parent dir doesn't exist (`/nonexistent/cache.json`) and assert the failure happens at the tmp-file creation step rather than the rename, leaving any prior state untouched.
- Determinism: save the same cache twice (with `BTreeMap`-backed ordering), file bytes are identical. `let bytes1 = save(); let bytes2 = save(); assert_eq!(bytes1, bytes2);`

**Verification:**

Run: `cd <project-root> && cargo test --lib adapters::cache`
Expected: all cases pass. The atomicity test should not flake; if it does on first attempt, switch to the "nonexistent parent" variant.

**Commit:** `Phase 4: Cache::save_atomic + BTreeMap-backed deterministic serialization`
<!-- END_TASK_7 -->

<!-- END_SUBCOMPONENT_C -->

<!-- START_SUBCOMPONENT_D (task 8) -->

<!-- START_TASK_8 -->
### Task 8: Wire adapters module, run full verification, commit

**Files:**
- Modify: `<project-root>/src/adapters/mod.rs`

**Implementation:**

```rust
//! Impure adapter shell — file IO, HTTP, JSON cache.
//!
//! Trait surfaces (`Resolver`, `FeatureSource`) land in Phase 5/6 once
//! the first impls are written. Phase 4 establishes the IO primitives
//! and the cache that the trait impls will read/write through.

pub mod cache;
pub mod csv_io;

pub use cache::{Cache, CacheFile, CACHE_VERSION};
pub use csv_io::{read_input, write_output, write_unresolved, Unresolved};
```

Add `tempfile = "=3.13.0"` to `[dev-dependencies]` in `Cargo.toml` (used by Tasks 2/3/4/7 tests). If the version doesn't resolve, fall back to the latest 3.x.

Final verification:

```bash
cd <project-root>
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

All four exit 0. `cargo test` should report tests covering: `errors::*`, `adapters::csv_io::tests::*`, `adapters::cache::tests::*`, plus everything from Phases 2–3.

If any prior task wasn't committed, stage and commit:
```bash
cd <project-root>
git add src/adapters/ src/errors.rs Cargo.toml Cargo.lock
git status
git commit -m "Phase 4: adapters module re-exports + tempfile dev-dep"
```

**Verification:**

Run: `cd <project-root> && git status`
Expected: `nothing to commit, working tree clean`.
<!-- END_TASK_8 -->

<!-- END_SUBCOMPONENT_D -->

---

## Phase 4 Done When

- `cargo test` exercises every case in the AC coverage table above.
- `Cache::save_atomic` is verified by both a round-trip test and an atomicity test (mid-write failure leaves existing file intact).
- Cache serialization uses `BTreeMap` so output is byte-deterministic across runs.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --release` all exit 0.
- Errors include source paths AND line numbers in their `Display` output.
- All `pub` items have doc comments.
- Commits on `playlist-arc-v1` whose subjects start with `Phase 4:`.

## Risk callouts

- **HashMap → BTreeMap determinism.** Easy to miss. `cargo test` won't catch non-determinism unless you explicitly compare two serialized outputs. The Phase 7 determinism test (AC5.1) will catch this end-to-end, but discovering the bug there is more expensive than fixing it here.
- **macOS read-only directory test.** The atomicity test may behave differently on macOS vs. Linux. Use the "nonexistent parent" fallback if needed; either variant proves the atomicity property.
- **`unwrap()` in `read_input`.** The `.unwrap()` on already-validated indices is technically allowed-but-flagged by the project rule. Use `.expect("validated above by missing check")` with a justifying message, or restructure to bubble the None.
