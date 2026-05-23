//! Simulated-annealing loop over playlist permutations.
//!
//! The loop is pure: takes a seeded `R: Rng`, returns a permutation.
//! No tokio, no IO, no logging at iteration granularity (one info!
//! event at start and end is acceptable; per-iteration logging is not).

use crate::algo::cost::CostContext;
use rand::{seq::SliceRandom, Rng};

/// Helper to generate a random usize in the range [0, n)
/// Note: uses modulo, which has negligible bias for n ≤ 100 (< 1 part in 4e7).
#[inline]
fn gen_range_usize<R: Rng + ?Sized>(rng: &mut R, n: usize) -> usize {
    let val = rng.next_u32() as usize;
    val % n
}

/// Helper to generate a random f32 in [0, 1)
/// Note: (u32::MAX as f32 + 1.0) rounds to 2^32 in f32, so this divides by 2^32
/// and produces values in [0, 1).
#[inline]
fn gen_f32<R: Rng + ?Sized>(rng: &mut R) -> f32 {
    let val = rng.next_u32();
    (val as f32) / (u32::MAX as f32 + 1.0)
}

/// Configuration for the simulated annealing optimizer.
///
/// Tuning parameters include cooling rate, iteration budget, restart strategy,
/// and pilot calibration settings for initial temperature selection.
#[derive(Debug, Clone)]
pub struct AnnealConfig {
    /// Cooling rate. α < 1; default 0.998.
    pub alpha: f32,
    /// Total iterations across all restarts. Default 100_000.
    pub iterations: usize,
    /// Number of independent restarts; best ordering across restarts is
    /// returned. Default 2.
    pub restarts: u8,
    /// Pilot-calibration iterations (used to set T₀ — see below).
    /// Default 500.
    pub pilot_iterations: usize,
    /// Pilot-calibration target acceptance rate; T₀ is chosen so this
    /// fraction of worsening moves would be accepted at start.
    /// Default 0.4.
    pub pilot_target_acceptance: f32,
}

impl Default for AnnealConfig {
    fn default() -> Self {
        Self {
            alpha: 0.998,
            iterations: 100_000,
            restarts: 2,
            pilot_iterations: 500,
            pilot_target_acceptance: 0.4,
        }
    }
}

/// Run simulated annealing with delta-cost. Returns the best permutation found.
///
/// Determinism contract: given identical `initial`, identical `&[Track]` in
/// `ctx`, identical `AnnealConfig`, and the same `R` state, repeated calls
/// produce identical output. Use `ChaCha20Rng::seed_from_u64(seed)` to get
/// a reproducible `R`.
pub fn optimize<R: Rng + ?Sized>(
    initial: Vec<usize>,
    ctx: &CostContext<'_>,
    config: &AnnealConfig,
    rng: &mut R,
) -> Vec<usize> {
    let n = initial.len();
    if n == 0 {
        return vec![];
    }

    // 1. Pilot calibration: sample `pilot_iterations` random 2-swaps,
    //    record positive delta_costs, choose T₀ such that
    //    exp(-mean_positive_delta / T₀) ≈ pilot_target_acceptance.
    //    Concretely: T₀ = -mean_positive_delta / ln(pilot_target_acceptance).
    let mut positive_deltas = Vec::new();
    let mut pilot_buf = initial.clone();

    for _ in 0..config.pilot_iterations {
        // Sample two different random indices
        let a = gen_range_usize(rng, n);
        let b = loop {
            let x = gen_range_usize(rng, n);
            if x != a {
                break x;
            }
        };
        let delta = ctx.delta_cost(&mut pilot_buf, a, b);
        if delta > 0.0 {
            positive_deltas.push(delta);
        }
    }

    let t0 = if positive_deltas.is_empty() {
        0.01
    } else {
        let mean_positive_delta =
            positive_deltas.iter().sum::<f32>() / positive_deltas.len() as f32;
        let computed_t0 = -mean_positive_delta / config.pilot_target_acceptance.ln();
        computed_t0.max(0.01)
    };

    tracing::info!(
        "Simulated annealing: T₀={:.6}, alpha={}, iterations={}, restarts={}",
        t0,
        config.alpha,
        config.iterations,
        config.restarts
    );

    let mut best_ordering = initial.clone();
    let mut best_cost = ctx.total_cost(&best_ordering);
    let initial_cost = best_cost;

    let iters_per_restart = config.iterations / (config.restarts as usize);

    // 2. For each restart
    for restart in 0..config.restarts {
        let mut ordering = if restart == 0 {
            initial.clone()
        } else {
            // Shuffle for restarts > 0
            let mut shuffled = initial.clone();
            shuffled.shuffle(rng);
            shuffled
        };

        let mut current_cost = ctx.total_cost(&ordering);

        // 3. For each iteration
        for iter in 0..iters_per_restart {
            // Geometric cooling: T = T₀ * α^iter
            let t = t0 * config.alpha.powf(iter as f32);

            // Pick random a, b in 0..n, a != b
            let a = gen_range_usize(rng, n);
            let b = loop {
                let x = gen_range_usize(rng, n);
                if x != a {
                    break x;
                }
            };

            let delta = ctx.delta_cost(&mut ordering, a, b);

            // Accept if delta < 0 OR rng.gen::<f32>() < exp(-delta / T)
            let rand_val = gen_f32(rng);
            let accept = delta < 0.0 || (rand_val < (-delta / t).exp());

            if accept {
                ordering.swap(a, b);
                current_cost += delta;

                if current_cost < best_cost {
                    best_cost = current_cost;
                    best_ordering = ordering.clone();
                }
            }
        }
    }

    tracing::info!(
        "Simulated annealing complete: {} iterations × {} restarts, initial cost {:.2}, final cost {:.2}",
        iters_per_restart,
        config.restarts,
        initial_cost,
        best_cost
    );

    best_ordering
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algo::test_support::{synthetic_tracks, synthetic_tracks_with_artists};
    use crate::algo::{CamelotTable, CostContext, CostWeights, EnergyArc};
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    #[test]
    fn smoke_test_5_track_completes_quickly() {
        let tracks = synthetic_tracks(5, 42);
        let ctx = CostContext {
            tracks: &tracks,
            weights: crate::algo::CostWeights::default(),
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };
        let initial = (0..5).collect();
        let config = AnnealConfig {
            iterations: 100,
            restarts: 1,
            ..Default::default()
        };
        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let result = optimize(initial, &ctx, &config, &mut rng);
        assert_eq!(result.len(), 5);
        for i in 0..5 {
            assert!(result.contains(&i));
        }
    }

    proptest! {
        #[test]
        fn permutation_property(
            n in 5usize..=20,
            seed in any::<u64>(),
        ) {
            let tracks = synthetic_tracks(n, seed);
            let ctx = CostContext {
                tracks: &tracks,
                weights: crate::algo::CostWeights::default(),
                arc: EnergyArc,
                camelot_table: CamelotTable::new(),
            };
            let initial: Vec<usize> = (0..n).collect();
            let config = AnnealConfig {
                iterations: 500,
                restarts: 1,
                ..Default::default()
            };
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let result = optimize(initial, &ctx, &config, &mut rng);

            // Check that result is a permutation of 0..n
            let mut sorted_result = result.clone();
            sorted_result.sort();
            let expected: Vec<usize> = (0..n).collect();
            prop_assert_eq!(sorted_result, expected);
        }
    }

    #[test]
    fn determinism_same_seed_produces_identical_output() {
        let tracks = synthetic_tracks(10, 42);
        let ctx = CostContext {
            tracks: &tracks,
            weights: crate::algo::CostWeights::default(),
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };
        let initial: Vec<usize> = (0..10).collect();
        let config = AnnealConfig::default();

        let mut rng1 = ChaCha20Rng::seed_from_u64(42);
        let result1 = optimize(initial.clone(), &ctx, &config, &mut rng1);

        let mut rng2 = ChaCha20Rng::seed_from_u64(42);
        let result2 = optimize(initial.clone(), &ctx, &config, &mut rng2);

        assert_eq!(
            result1, result2,
            "Determinism test failed: different outputs for same seed"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 10, .. ProptestConfig::default() })]
        #[test]
        fn improvement_on_bad_initial(seed in any::<u64>()) {
            // Create synthetic tracks where artists are pre-clustered in the input array.
            // The sequential initial ordering (track indices [0,1,...,n-1]) exposes
            // this clustering at the position level, creating a bad starting cost.
            // The optimizer should improve upon this.
            let mut tracks = synthetic_tracks(20, seed);
            for (i, track) in tracks.iter_mut().enumerate() {
                track.query.artist = format!("Artist {}", i / 5);
            }
            let ctx = CostContext {
                tracks: &tracks,
                weights: crate::algo::CostWeights::default(),
                arc: EnergyArc,
                camelot_table: CamelotTable::new(),
            };

            let initial: Vec<usize> = (0..20).collect();
            let initial_cost = ctx.total_cost(&initial);

            let config = AnnealConfig::default();
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let result = optimize(initial, &ctx, &config, &mut rng);
            let result_cost = ctx.total_cost(&result);

            // Expect improvement (result_cost < initial_cost)
            prop_assert!(result_cost < initial_cost,
                "expected improvement: {} < {}", result_cost, initial_cost);
        }
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn perf_sanity_1000_iter_40_track_under_100ms() {
        use std::time::Instant;
        let tracks = synthetic_tracks(40, 1234);
        let ctx = CostContext {
            tracks: &tracks,
            weights: crate::algo::CostWeights::default(),
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };
        let mut rng = ChaCha20Rng::seed_from_u64(1234);
        let cfg = AnnealConfig {
            iterations: 1000,
            restarts: 1,
            pilot_iterations: 100,
            ..Default::default()
        };
        let start = Instant::now();
        let _ = optimize((0..40).collect(), &ctx, &cfg, &mut rng);
        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() < 100,
            "perf budget exceeded: {:?}",
            elapsed
        );
    }

    #[test]
    fn artist_spacing_respects_heavy_weight() {
        // Test that with a heavy artist_clash weight, we get significant improvement
        let tracks = synthetic_tracks_with_artists(12, 3, 42);
        let weights = crate::algo::CostWeights {
            artist_clash: 500.0,
            ..Default::default()
        };
        let config = AnnealConfig {
            iterations: 200_000,
            ..Default::default()
        };
        let ctx = CostContext {
            tracks: &tracks,
            weights,
            arc: EnergyArc,
            camelot_table: CamelotTable::new(),
        };
        let initial: Vec<usize> = (0..12).collect();
        let initial_cost = ctx.total_cost(&initial);

        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let result = optimize(initial, &ctx, &config, &mut rng);
        let final_cost = ctx.total_cost(&result);

        // Verify substantial cost improvement
        assert!(
            final_cost < initial_cost * 0.7,
            "Expected significant improvement from {:.0} to {:.0}",
            initial_cost,
            final_cost
        );

        // Count clashes in result
        let mut clash_count = 0;
        for i in 0..result.len() {
            for j in (i + 1)..(i + 5).min(result.len()) {
                let a_i = &tracks[result[i]].query.artist;
                let a_j = &tracks[result[j]].query.artist;
                if a_i.eq_ignore_ascii_case(a_j) {
                    clash_count += 1;
                }
            }
        }
        // With weight=500 and enough iterations, we should achieve strong clash reduction
        let initial_clashes = 8; // sequential ordering has ~8 clashes within window
        assert!(
            clash_count <= 5,
            "Expected strong constraint enforcement, got {} clashes (initial had ~{})",
            clash_count,
            initial_clashes
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 8, .. ProptestConfig::default() })]
        #[test]
        fn artist_spacing_respected_default_window(seed in any::<u64>()) {
            // Build 20 synthetic tracks with 5 distinct artists (distributed round-robin).
            // This is feasible: with 5 artists and 20 tracks, we can achieve zero artist
            // clashes within the window of 4 (average spacing of 4 between same-artist tracks).
            let tracks = synthetic_tracks_with_artists(20, 5, seed);
            let ctx = CostContext {
                tracks: &tracks,
                weights: CostWeights::default(), // artist_window=4, artist_clash=500.0
                arc: EnergyArc,
                camelot_table: CamelotTable::new(),
            };
            let initial: Vec<usize> = (0..20).collect();
            let config = AnnealConfig {
                iterations: 200_000,  // Sufficient iterations to find zero-clash solution
                ..Default::default()
            };
            let mut rng = ChaCha20Rng::seed_from_u64(seed);
            let result = optimize(initial, &ctx, &config, &mut rng);

            // Assert the invariant: no two tracks within 4 positions share an artist.
            for i in 0..result.len() {
                for j in (i+1)..(i+5).min(result.len()) {
                    let a_i = &tracks[result[i]].query.artist;
                    let a_j = &tracks[result[j]].query.artist;
                    prop_assert!(!a_i.eq_ignore_ascii_case(a_j),
                        "clash at positions {} and {}: {:?} vs {:?}", i, j, a_i, a_j);
                }
            }
        }
    }
}
