//! Target per-position energy curve.
//!
//! The default curve is a fixed asymmetric shape that peaks near
//! position 0.68 and tapers to ~0.3 at both ends. It is intentionally
//! parameter-free in v1 — the design plan removed anchor/banger
//! pacing knobs to keep v1 small.

use crate::domain::Normalized;

pub struct EnergyArc;

impl EnergyArc {
    /// Target energy in [0, 1] for position `i` out of `n` total tracks.
    /// `n == 0` or `n == 1` returns the curve's peak (0.68) — there is
    /// no meaningful arc with one track, so we surface the peak as a
    /// degenerate fallback that keeps the chart rendering well.
    pub fn target(&self, position: usize, n: usize) -> Normalized {
        if n == 0 || n == 1 {
            // Fallback: return the peak value at t=0.5
            let peak = raw_target(0.5);
            Normalized::clamp(peak)
        } else {
            let t = (position as f32 + 0.5) / n as f32;
            let value = raw_target(t);
            Normalized::clamp(value)
        }
    }

    /// Squared-error deviation between `actual` and `target(position, n)`.
    /// The cost function uses this as a per-position term.
    pub fn deviation_cost(&self, position: usize, n: usize, actual: Normalized) -> f32 {
        let target = self.target(position, n);
        let diff = actual.get() - target.get();
        diff * diff
    }
}

impl Default for EnergyArc {
    fn default() -> Self {
        EnergyArc
    }
}

/// Raw target value before normalization.
/// Formula: 4*t^2*(1-t), scaled so max == 1.
/// Peaks at t = 2/3 ≈ 0.667
fn raw_target(t: f32) -> f32 {
    let raw = 4.0 * t * t * (1.0 - t);
    const SCALE: f32 = 27.0 / 16.0; // 1.0 / (16/27), where 16/27 is the peak value
    let scaled = raw * SCALE;
    scaled.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::{proptest, prelude::*};

    #[test]
    fn target_peak_fallback_above_threshold() {
        let arc = EnergyArc;
        let peak_n1 = arc.target(0, 1).get();
        assert!(peak_n1 > 0.8, "peak at n=1 should be > 0.8, got {}", peak_n1);
    }

    #[test]
    fn target_first_of_ten_small() {
        let arc = EnergyArc;
        let val = arc.target(0, 10).get();
        assert!(val < 0.2, "target(0, 10) should be < 0.2, got {}", val);
    }

    #[test]
    fn target_near_peak_high() {
        let arc = EnergyArc;
        let val = arc.target(7, 10).get();
        assert!(val >= 0.94, "target(7, 10) should be >= 0.94, got {}", val);
    }

    #[test]
    fn target_last_of_ten_tapers() {
        let arc = EnergyArc;
        let val = arc.target(9, 10).get();
        assert!(val < 0.5, "target(9, 10) should be < 0.5, got {}", val);
    }

    #[test]
    fn deviation_cost_always_nonnegative() {
        let arc = EnergyArc;
        for position in 0..10 {
            for n in 1..=10 {
                let actual = Normalized::clamp(0.5);
                let cost = arc.deviation_cost(position, n, actual);
                assert!(cost >= 0.0, "deviation_cost should be >= 0");
            }
        }
    }

    #[test]
    fn deviation_cost_zero_when_actual_equals_target() {
        let arc = EnergyArc;
        for position in 0..10 {
            for n in 1..=10 {
                let target = arc.target(position, n);
                let cost = arc.deviation_cost(position, n, target);
                assert!(
                    cost < f32::EPSILON * 10.0,
                    "deviation_cost should be ~0 when actual == target, got {}",
                    cost
                );
            }
        }
    }

    proptest! {
        #[test]
        fn target_always_in_range(n in 1usize..=200, i in 0usize..200) {
            let arc = EnergyArc;
            let position = if i < n { i } else { n - 1 };
            let val = arc.target(position, n).get();
            prop_assert!((0.0..=1.0).contains(&val), "target({}, {}) out of range: {}", position, n, val);
        }
    }
}
