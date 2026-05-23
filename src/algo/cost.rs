//! Weighted cost function over a playlist ordering.
//!
//! The cost is a sum of:
//! 1. Per-position arc deviation (EnergyArc).
//! 2. Pairwise Camelot distance over adjacent and near-adjacent tracks.
//! 3. Tempo delta between adjacent tracks.
//! 4. Energy jump between adjacent tracks.
//! 5. Artist-clash penalty within a window — a high-weight term that
//!    encodes the artist-spacing hard constraint as a soft penalty
//!    large enough that SA naturally avoids it.

use crate::algo::{CamelotTable, EnergyArc};
use crate::domain::{CamelotCode, Track};

/// Per-term breakdown of the cost over an ordering. Returned by
/// `CostContext::cost_breakdown`. Lives in `algo/cost.rs` (not
/// `cli/report.rs`) so that the algorithm owns the cost taxonomy.
#[derive(Debug, Clone, Copy)]
pub struct CostBreakdown {
    pub arc: f32,
    pub camelot: f32,
    pub tempo: f32,
    pub energy: f32,
    pub artist: f32,
}

/// Weights for each term in the cost function.
#[derive(Debug, Clone)]
pub struct CostWeights {
    /// Weight for per-position arc deviation.
    pub arc_deviation: f32,
    /// Weight for Camelot harmonic distance between adjacent tracks.
    pub camelot_distance: f32,
    /// Weight for tempo delta (BPM difference).
    pub tempo_delta: f32,
    /// Weight for energy jump between adjacent tracks.
    pub energy_jump: f32,
    /// Weight for artist-clash penalty (high to enforce hard constraint softly).
    pub artist_clash: f32,
    /// Window over which the artist-clash term applies (default 4).
    /// `0` disables the term entirely.
    pub artist_window: u8,
}

impl Default for CostWeights {
    fn default() -> Self {
        Self {
            arc_deviation: 1.0,
            camelot_distance: 0.3,
            tempo_delta: 0.02,
            energy_jump: 0.5,
            // Heavy weight so SA naturally pushes clashes out of view.
            // Must be at least 10× the arc term to dominate at SA's
            // typical temperatures. Default 50.0 with window=4.
            artist_clash: 50.0,
            artist_window: 4,
        }
    }
}

/// Context for computing costs over a playlist ordering.
pub struct CostContext<'a> {
    /// Slice of all tracks in the playlist.
    pub tracks: &'a [Track],
    /// Weight configuration.
    pub weights: CostWeights,
    /// Energy arc curve for per-position deviation.
    pub arc: EnergyArc,
    /// Camelot distance table.
    pub camelot_table: CamelotTable,
}

impl<'a> CostContext<'a> {
    /// Per-term cost decomposition. Σ of all five terms equals
    /// `total_cost(ordering)` (within f32 rounding).
    pub fn cost_breakdown(&self, ordering: &[usize]) -> CostBreakdown {
        let n = ordering.len();
        let mut arc = 0.0;
        let mut camelot = 0.0;
        let mut tempo = 0.0;
        let mut energy = 0.0;
        let mut artist = 0.0;

        // Arc deviation per position
        for (pos, &track_idx) in ordering.iter().enumerate() {
            let energy_val = self.tracks[track_idx].features.energy;
            arc += self.weights.arc_deviation * self.arc.deviation_cost(pos, n, energy_val);
        }

        // Pairwise terms for adjacent pairs (distance-1)
        for i in 0..n.saturating_sub(1) {
            let track_i = &self.tracks[ordering[i]];
            let track_j = &self.tracks[ordering[i + 1]];
            let (c, t, e) = self.pairwise_term_breakdown(track_i, track_j);
            camelot += c;
            tempo += t;
            energy += e;
        }

        // Pairwise terms for distance-2 pairs (scaled by 0.5)
        for i in 0..n.saturating_sub(2) {
            let track_i = &self.tracks[ordering[i]];
            let track_k = &self.tracks[ordering[i + 2]];
            let (c, t, e) = self.pairwise_term_breakdown(track_i, track_k);
            camelot += 0.5 * c;
            tempo += 0.5 * t;
            energy += 0.5 * e;
        }

        // Artist-clash penalties
        if self.weights.artist_window > 0 {
            for i in 0..n {
                for j in (i + 1)..=(i + self.weights.artist_window as usize) {
                    if j >= n {
                        break;
                    }
                    let artist_i = &self.tracks[ordering[i]].query.artist;
                    let artist_j = &self.tracks[ordering[j]].query.artist;
                    if artist_i.eq_ignore_ascii_case(artist_j) {
                        artist += self.weights.artist_clash;
                    }
                }
            }
        }

        CostBreakdown { arc, camelot, tempo, energy, artist }
    }

    /// Full cost over `ordering[..]`.
    ///
    /// Computes the sum of:
    /// - Per-position arc deviation
    /// - Pairwise terms for distance-1 neighbors
    /// - Pairwise terms for distance-2 neighbors (scaled by 0.5)
    /// - Artist-clash penalties for pairs within artist_window
    pub fn total_cost(&self, ordering: &[usize]) -> f32 {
        let b = self.cost_breakdown(ordering);
        b.arc + b.camelot + b.tempo + b.energy + b.artist
    }

    /// Incremental cost change for swapping `ordering[a]` and `ordering[b]`.
    ///
    /// Returns Δ such that:
    ///   total_cost(ordering_after_swap) == total_cost(ordering_before_swap) + Δ
    ///
    /// This must NOT iterate the full ordering. Only the terms touching
    /// positions `a`, `b`, and their neighbors within max(2, artist_window)
    /// need re-computation.
    ///
    /// Zero allocations: computes costs by directly swapping in place, computing
    /// after-costs, then swapping back to restore the original ordering.
    pub fn delta_cost(&self, ordering: &mut [usize], a: usize, b: usize) -> f32 {
        if a == b {
            return 0.0;
        }

        let n = ordering.len();
        if a >= n || b >= n {
            return 0.0;
        }

        let (min_pos, max_pos) = if a < b { (a, b) } else { (b, a) };

        // Compute the range of positions affected
        let window = std::cmp::max(2, self.weights.artist_window as usize);
        let start = min_pos.saturating_sub(window);
        let end = (max_pos + window + 1).min(n);

        // Compute "before" subtotal for affected terms
        let before = self.compute_affected_cost(ordering, start, end);

        // Perform swap (in place)
        ordering.swap(a, b);

        // Compute "after" subtotal for affected terms
        let after = self.compute_affected_cost(ordering, start, end);

        // Restore original ordering by swapping back
        ordering.swap(a, b);

        after - before
    }

    /// Compute the cost contribution for positions in [start, end).
    /// This includes arc deviation, pairwise terms, and artist clashes
    /// that involve any position in this range.
    fn compute_affected_cost(&self, ordering: &[usize], start: usize, end: usize) -> f32 {
        let n = ordering.len();
        let mut cost = 0.0;
        let clamped_end = end.min(n);

        // Arc deviation for positions in range
        for (i, &track_idx) in ordering[start..clamped_end].iter().enumerate() {
            let pos = start + i;
            let energy = self.tracks[track_idx].features.energy;
            cost += self.weights.arc_deviation * self.arc.deviation_cost(pos, n, energy);
        }

        // Pairwise distance-1 terms where at least one endpoint is in range
        for i in start.saturating_sub(1)..clamped_end {
            if i + 1 < n {
                let track_i = &self.tracks[ordering[i]];
                let track_j = &self.tracks[ordering[i + 1]];
                cost += self.pairwise_term(track_i, track_j);
            }
        }

        // Pairwise distance-2 terms where at least one endpoint is in range
        for i in start.saturating_sub(2)..clamped_end {
            if i + 2 < n {
                let track_i = &self.tracks[ordering[i]];
                let track_k = &self.tracks[ordering[i + 2]];
                cost += 0.5 * self.pairwise_term(track_i, track_k);
            }
        }

        // Artist-clash penalties involving positions in range
        if self.weights.artist_window > 0 {
            for i in start..clamped_end {
                for j in (i + 1)..=(i + self.weights.artist_window as usize) {
                    if j >= n {
                        break;
                    }
                    let artist_i = &self.tracks[ordering[i]].query.artist;
                    let artist_j = &self.tracks[ordering[j]].query.artist;
                    if artist_i.eq_ignore_ascii_case(artist_j) {
                        cost += self.weights.artist_clash;
                    }
                }
            }
        }

        cost
    }

    /// Compute the pairwise cost term between two adjacent (or nearby) tracks.
    fn pairwise_term(&self, track_i: &Track, track_j: &Track) -> f32 {
        let (c, t, e) = self.pairwise_term_breakdown(track_i, track_j);
        c + t + e
    }

    /// Compute the pairwise cost term breakdown into (camelot, tempo, energy).
    fn pairwise_term_breakdown(&self, track_i: &Track, track_j: &Track) -> (f32, f32, f32) {
        let camelot_i = CamelotCode::from((track_i.features.key, track_i.features.mode));
        let camelot_j = CamelotCode::from((track_j.features.key, track_j.features.mode));

        let camelot_cost =
            self.weights.camelot_distance * self.camelot_table.distance(camelot_i, camelot_j);
        let tempo_cost = self.weights.tempo_delta
            * (track_i.features.tempo.get() - track_j.features.tempo.get()).abs();
        let energy_cost = self.weights.energy_jump
            * (track_i.features.energy.get() - track_j.features.energy.get()).abs();

        (camelot_cost, tempo_cost, energy_cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algo::test_support::synthetic_tracks;
    use proptest::prelude::*;

    #[test]
    fn cost_weights_default_has_correct_values() {
        let weights = CostWeights::default();
        assert_eq!(weights.arc_deviation, 1.0);
        assert_eq!(weights.camelot_distance, 0.3);
        assert_eq!(weights.tempo_delta, 0.02);
        assert_eq!(weights.energy_jump, 0.5);
        assert_eq!(weights.artist_clash, 50.0);
        assert_eq!(weights.artist_window, 4);
    }

    #[test]
    fn cost_weights_with_disabled_artist_window() {
        let weights = CostWeights {
            artist_window: 0,
            ..Default::default()
        };
        assert_eq!(weights.artist_window, 0);
    }

    #[test]
    fn cost_context_can_be_constructed() {
        let tracks = synthetic_tracks(5, 42);
        let ctx = CostContext {
            tracks: &tracks,
            weights: CostWeights::default(),
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };
        assert_eq!(ctx.tracks.len(), 5);
    }

    #[test]
    fn total_cost_single_track_returns_only_arc_deviation() {
        let tracks = synthetic_tracks(1, 42);
        let ctx = CostContext {
            tracks: &tracks,
            weights: CostWeights::default(),
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };

        let ordering = vec![0];
        let cost = ctx.total_cost(&ordering);

        // Should only have arc deviation, no pairwise or artist terms
        let expected =
            ctx.weights.arc_deviation * ctx.arc.deviation_cost(0, 1, tracks[0].features.energy);
        assert!((cost - expected).abs() < 1e-5);
    }

    #[test]
    fn total_cost_same_artist_adjacent_higher_than_separated() {
        let mut tracks = synthetic_tracks(6, 42);
        // Make tracks 0 and 1 have the same artist
        tracks[0].query.artist = "CommonArtist".to_string();
        tracks[1].query.artist = "CommonArtist".to_string();

        let ctx = CostContext {
            tracks: &tracks,
            weights: CostWeights::default(),
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };

        // Cost with them adjacent (positions 0, 1)
        let ordering_adjacent = vec![0, 1, 2, 3, 4, 5];
        let cost_adjacent = ctx.total_cost(&ordering_adjacent);

        // Cost with them 5 apart (positions 0, 5 - outside artist_window of 4)
        let ordering_separated = vec![0, 2, 3, 4, 5, 1];
        let cost_separated = ctx.total_cost(&ordering_separated);

        // Adjacent should be higher due to artist_clash
        assert!(
            cost_adjacent > cost_separated,
            "adjacent cost {} should be > separated cost {}",
            cost_adjacent,
            cost_separated
        );
        let diff = cost_adjacent - cost_separated;
        assert!(
            diff >= ctx.weights.artist_clash - 1.0,
            "cost difference {} should be at least artist_clash {}",
            diff,
            ctx.weights.artist_clash
        );
    }

    #[test]
    fn delta_cost_no_op_swap_is_zero() {
        let tracks = synthetic_tracks(5, 42);
        let ctx = CostContext {
            tracks: &tracks,
            weights: CostWeights::default(),
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };

        let mut ordering = vec![0, 1, 2, 3, 4];
        let delta = ctx.delta_cost(&mut ordering, 2, 2);
        assert_eq!(delta, 0.0);
    }

    #[test]
    fn delta_cost_swap_is_symmetric() {
        let tracks = synthetic_tracks(5, 42);
        let ctx = CostContext {
            tracks: &tracks,
            weights: CostWeights::default(),
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };

        let mut ordering_ab = vec![0, 1, 2, 3, 4];
        let delta_ab = ctx.delta_cost(&mut ordering_ab, 1, 3);
        let mut ordering_ba = vec![0, 1, 2, 3, 4];
        let delta_ba = ctx.delta_cost(&mut ordering_ba, 3, 1);
        assert_eq!(delta_ab, delta_ba);
    }

    proptest! {
        #[test]
        fn delta_cost_equals_total_cost_diff(
            n in 5usize..=30,
            seed in any::<u64>(),
            a_offset in 0usize..30,
            b_offset in 0usize..30,
        ) {
            let tracks: Vec<Track> = synthetic_tracks(n, seed);
            let ctx = CostContext {
                tracks: &tracks,
                weights: CostWeights::default(),
                arc: EnergyArc,
                camelot_table: CamelotTable::new(),
            };
            let mut ordering: Vec<usize> = (0..n).collect();

            // Pick two different positions
            let a = a_offset % n;
            let b = (b_offset + 1) % n; // +1 ensures b != a
            prop_assume!(a != b);

            let before = ctx.total_cost(&ordering);
            let delta = ctx.delta_cost(&mut ordering, a, b);
            ordering.swap(a, b);
            let after = ctx.total_cost(&ordering);

            prop_assert!(
                (after - (before + delta)).abs() < 1e-3,
                "delta {} vs actual diff {}",
                delta,
                after - before
            );
        }

        #[test]
        fn breakdown_sum_equals_total_cost(
            n in 5usize..=20,
            seed in any::<u64>(),
        ) {
            let tracks = synthetic_tracks(n, seed);
            let ctx = CostContext {
                tracks: &tracks,
                weights: CostWeights::default(),
                arc: EnergyArc,
                camelot_table: CamelotTable::new(),
            };
            let ordering: Vec<usize> = (0..n).collect();
            let total = ctx.total_cost(&ordering);
            let b = ctx.cost_breakdown(&ordering);
            let sum = b.arc + b.camelot + b.tempo + b.energy + b.artist;
            prop_assert!(
                (total - sum).abs() < 1e-3,
                "sum {} vs total {}", sum, total
            );
        }
    }
}
