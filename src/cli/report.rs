//! Summary report printed to stdout at end of run.
//!
//! Format (example):
//! ```text
//! Summary
//!   resolved:     38
//!   unresolved:    2  (see unresolved.csv)
//!   seed:         42  (supplied)
//!   total cost:   before  12.345    after  7.891
//!   arc dev:      before   8.000    after  4.123
//!   cost terms (after):
//!     arc        4.123
//!     camelot    1.234
//!     tempo      0.567
//!     energy     0.890
//!     artist     1.077
//!   artist clashes remaining: 0
//! ```

use crate::domain::Track;
use std::fmt::Write;
use std::path::Path;

pub struct SummaryInputs<'a> {
    pub resolved: usize,
    pub unresolved: usize,
    pub unresolved_path: &'a Path,
    pub seed: u64,
    pub seed_was_supplied: bool,
    pub before_cost: f32,
    pub after_cost: f32,
    pub before_arc_dev: f32,
    pub after_arc_dev: f32,
    pub cost_breakdown: CostBreakdown,
    pub remaining_clashes: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct CostBreakdown {
    pub arc: f32,
    pub camelot: f32,
    pub tempo: f32,
    pub energy: f32,
    pub artist: f32,
}

pub fn format_summary(s: &SummaryInputs<'_>) -> String {
    let mut out = String::new();
    // `write!`/`writeln!` against a `String` is infallible (the
    // underlying `fmt::Write for String` impl cannot fail). We use
    // `let _ = ...` throughout instead of `.unwrap()` — project rule:
    // no `unwrap()` outside tests/main.
    let _ = writeln!(out, "Summary");
    let _ = writeln!(out, "  resolved:    {:>3}", s.resolved);
    if s.unresolved > 0 {
        let _ = writeln!(
            out,
            "  unresolved:  {:>3}  (see {})",
            s.unresolved,
            s.unresolved_path.display()
        );
    } else {
        let _ = writeln!(out, "  unresolved:  {:>3}", s.unresolved);
    }
    let seed_origin = if s.seed_was_supplied {
        "supplied"
    } else {
        "system-time"
    };
    let _ = writeln!(out, "  seed:        {} ({})", s.seed, seed_origin);
    let _ = writeln!(
        out,
        "  total cost:  before {:>8.3}    after {:>8.3}",
        s.before_cost, s.after_cost
    );
    let _ = writeln!(
        out,
        "  arc dev:     before {:>8.3}    after {:>8.3}",
        s.before_arc_dev, s.after_arc_dev
    );
    let _ = writeln!(out, "  cost terms (after):");
    let _ = writeln!(out, "    arc        {:>8.3}", s.cost_breakdown.arc);
    let _ = writeln!(out, "    camelot    {:>8.3}", s.cost_breakdown.camelot);
    let _ = writeln!(out, "    tempo      {:>8.3}", s.cost_breakdown.tempo);
    let _ = writeln!(out, "    energy     {:>8.3}", s.cost_breakdown.energy);
    let _ = writeln!(out, "    artist     {:>8.3}", s.cost_breakdown.artist);
    let _ = writeln!(out, "  artist clashes remaining: {}", s.remaining_clashes);
    out
}

/// Helper to count artist clashes in an ordering (used by orchestration
/// for the `remaining_clashes` field; verifies AC3.4 reporting).
pub fn count_artist_clashes(tracks: &[Track], ordering: &[usize], window: u8) -> usize {
    if window == 0 {
        return 0;
    }
    let w = window as usize;
    let mut count = 0;
    for i in 0..ordering.len() {
        for j in (i + 1)..(i + w + 1).min(ordering.len()) {
            if tracks[ordering[i]]
                .query
                .artist
                .eq_ignore_ascii_case(&tracks[ordering[j]].query.artist)
            {
                count += 1;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn happy_path_summary_snapshot() {
        let s = SummaryInputs {
            resolved: 38,
            unresolved: 2,
            unresolved_path: &PathBuf::from("unresolved.csv"),
            seed: 42,
            seed_was_supplied: true,
            before_cost: 12.345,
            after_cost: 7.891,
            before_arc_dev: 8.0,
            after_arc_dev: 4.123,
            cost_breakdown: CostBreakdown {
                arc: 4.123,
                camelot: 1.234,
                tempo: 0.567,
                energy: 0.890,
                artist: 1.077,
            },
            remaining_clashes: 0,
        };
        insta::assert_snapshot!("summary_happy", format_summary(&s));
    }

    #[test]
    fn no_unresolved_omits_sidecar_pointer() {
        let s = SummaryInputs {
            resolved: 10,
            unresolved: 0,
            unresolved_path: &PathBuf::from("unresolved.csv"),
            seed: 0,
            seed_was_supplied: false,
            before_cost: 5.0,
            after_cost: 5.0,
            before_arc_dev: 5.0,
            after_arc_dev: 5.0,
            cost_breakdown: CostBreakdown {
                arc: 5.0,
                camelot: 0.0,
                tempo: 0.0,
                energy: 0.0,
                artist: 0.0,
            },
            remaining_clashes: 0,
        };
        let s = format_summary(&s);
        assert!(!s.contains("see unresolved.csv"));
    }

    #[test]
    fn count_artist_clashes_with_window_4_inclusive() {
        // 5 tracks alternating: A B A B A. Window=4 means any pair (i, j)
        // with `j - i in 1..=4` and same artist counts as a clash.
        // Same-artist pairs: (0,2)=A-A dist 2, (0,4)=A-A dist 4,
        // (1,3)=B-B dist 2, (2,4)=A-A dist 2. Four clashes total.
        let mk = |artist: &str| Track {
            query: crate::domain::TrackQuery::new("t", artist),
            id: crate::domain::TrackId::new("Z"),
            features: crate::domain::TrackFeatures::neutral(),
        };
        let tracks = vec![mk("A"), mk("B"), mk("A"), mk("B"), mk("A")];
        assert_eq!(count_artist_clashes(&tracks, &[0, 1, 2, 3, 4], 4), 4);
    }

    #[test]
    fn count_artist_clashes_window_3_excludes_distance_4() {
        // Boundary test: with window=3, distance 4 is OUTSIDE the window.
        // Same-artist pairs: (0,2)=A-A dist 2, (1,3)=B-B dist 2, (2,4)=A-A dist 2.
        // Three clashes total (excluding (0,4) which is distance 4).
        let mk = |artist: &str| Track {
            query: crate::domain::TrackQuery::new("t", artist),
            id: crate::domain::TrackId::new("Z"),
            features: crate::domain::TrackFeatures::neutral(),
        };
        let tracks = vec![mk("A"), mk("B"), mk("A"), mk("B"), mk("A")];
        assert_eq!(count_artist_clashes(&tracks, &[0, 1, 2, 3, 4], 3), 3);
    }
}
