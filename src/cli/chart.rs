//! ASCII energy-arc chart for stdout.
//!
//! Deterministic output: snapshot-tested via `insta`. Renders the
//! per-position energy of the reordered playlist as a vertical bar
//! chart of fixed width.

use crate::domain::Track;

/// Render an ASCII chart of energy vs position.
///
/// - `tracks[ordering[i]].features.energy` is the value plotted at column `i`.
/// - `width` is the chart height in rows (default 12 in `main.rs`).
///
/// Output format (example, width=12, n=5):
/// ```text
/// Energy arc
/// 1.0 |   #
/// 0.8 |   #
/// 0.6 |  ## #
/// 0.4 | ###  #
/// 0.2 |#####
/// 0.0 +-----
///      12345
/// ```
///
/// Empty input (n=0) renders just the header + axis labels.
pub fn render_arc(tracks: &[Track], ordering: &[usize], width: usize) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    // `write!`/`writeln!` against a `String` cannot fail (the underlying
    // `fmt::Write` impl for `String` is infallible). We use `let _ = ...`
    // throughout to silence the Result without `.unwrap()` — project rule:
    // no `unwrap()` outside tests/main.
    let _ = writeln!(out, "Energy arc");

    if ordering.is_empty() {
        let _ = writeln!(out, "(empty)");
        return out;
    }

    let n = ordering.len();
    let rows = width.max(2);
    // Build matrix top-down: row 0 is highest energy.
    for r in (0..rows).rev() {
        let threshold = (r as f32) / (rows as f32 - 1.0);
        let label = format!("{:.1}", threshold);
        let _ = write!(out, "{label} |");
        for &i in ordering {
            let e = tracks[i].features.energy.get();
            out.push(if e >= threshold { '#' } else { ' ' });
        }
        let _ = writeln!(out);
    }
    // Bottom axis.
    let _ = write!(out, "    +");
    for _ in 0..n {
        out.push('-');
    }
    let _ = writeln!(out);
    // Position labels (1-indexed, single digit; wrap if n > 9).
    let _ = write!(out, "     ");
    for i in 1..=n {
        // unwrap_or('?') is a total function, not an unwrap-on-Err.
        out.push(char::from_digit((i % 10) as u32, 10).unwrap_or('?'));
    }
    let _ = writeln!(out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Bpm, Mode, Normalized, PitchClass, Track, TrackFeatures, TrackId, TrackQuery,
    };

    fn track(energy: f32) -> Track {
        Track {
            query: TrackQuery::new("title", "artist"),
            id: TrackId::new("ZZZZ00000000"),
            features: TrackFeatures {
                tempo: Bpm::new(120.0).unwrap(),
                key: PitchClass::new(0).unwrap(),
                mode: Mode::Major,
                energy: Normalized::clamp(energy),
                danceability: Normalized::clamp(0.5),
                valence: Normalized::clamp(0.5),
                loudness: -10.0,
                acousticness: Normalized::clamp(0.5),
                instrumentalness: Normalized::clamp(0.0),
                liveness: Normalized::clamp(0.0),
                speechiness: Normalized::clamp(0.0),
            },
        }
    }

    #[test]
    fn renders_for_n_1() {
        let tracks = vec![track(0.5)];
        let s = render_arc(&tracks, &[0], 6);
        insta::assert_snapshot!("arc_n1", s);
    }

    #[test]
    fn renders_for_n_2() {
        let tracks = vec![track(0.2), track(0.9)];
        let s = render_arc(&tracks, &[0, 1], 6);
        insta::assert_snapshot!("arc_n2", s);
    }

    #[test]
    fn renders_for_n_3() {
        let tracks = vec![track(0.3), track(0.7), track(0.5)];
        let s = render_arc(&tracks, &[0, 1, 2], 6);
        insta::assert_snapshot!("arc_n3", s);
    }

    #[test]
    fn renders_for_n_10_climbing_arc() {
        let tracks: Vec<_> = (0..10).map(|i| track(i as f32 / 10.0)).collect();
        let s = render_arc(&tracks, &(0..10).collect::<Vec<_>>(), 6);
        insta::assert_snapshot!("arc_n10_climbing", s);
    }

    #[test]
    fn empty_renders_without_panic() {
        let s = render_arc(&[], &[], 6);
        assert!(s.contains("Energy arc"));
    }
}
