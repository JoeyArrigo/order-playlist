//! 24×24 harmonic-distance lookup over Camelot codes.
//!
//! Distances follow the DJ practitioner convention:
//! - 0: same code
//! - 1: adjacent on the same ring or relative-flip
//! - 2: ±2 on same ring, or adjacent on the other ring
//! - 4: anything further
//!
//! The table is symmetric (distance is a metric: d(a,b) == d(b,a)).

use crate::domain::CamelotCode;

/// A 24×24 lookup table of harmonic distances between all Camelot codes.
///
/// Indexes follow the convention set in Phase 2's `CamelotCode::index()`:
/// B-ring (major) → 0..=11, A-ring (minor) → 12..=23.
pub struct CamelotTable {
    distances: [[f32; 24]; 24],
}

impl CamelotTable {
    /// Build a new Camelot distance table from the harmonic rules.
    ///
    /// Distance follows the DJ practitioner convention:
    /// - 0: same code (e.g., 8A → 8A).
    /// - 1: adjacent on the same ring (8A → 9A, 8A → 7A) OR relative flip (8A → 8B).
    /// - 2: ±2 positions on the same ring (8A → 10A, 8A → 6A), OR adjacent on the *other*
    ///   ring (8A → 9B, 8A → 7B).
    /// - 4: ≥ 3 positions apart, on either ring.
    ///
    /// Distance wraps at 12 (circular ring).
    pub fn new() -> Self {
        let distances = std::array::from_fn(|a_idx| {
            std::array::from_fn(|b_idx| Self::compute_distance(a_idx, b_idx))
        });

        CamelotTable { distances }
    }

    /// Compute the harmonic distance between two Camelot codes by index.
    fn compute_distance(a_idx: usize, b_idx: usize) -> f32 {
        // Same code
        if a_idx == b_idx {
            return 0.0;
        }

        // Determine rings: 0..=11 is B ring (major), 12..=23 is A ring (minor)
        let a_ring = if a_idx < 12 { 'B' } else { 'A' };
        let b_ring = if b_idx < 12 { 'B' } else { 'A' };

        // Extract positions on the ring (0..=11)
        let a_pos = a_idx % 12;
        let b_pos = b_idx % 12;

        // Same ring?
        if a_ring == b_ring {
            // Distance on the ring (circular, so min of clockwise and counter-clockwise)
            let forward = (b_pos as i32 - a_pos as i32).rem_euclid(12);
            let backward = (a_pos as i32 - b_pos as i32).rem_euclid(12);
            let ring_distance = forward.min(backward);

            return match ring_distance {
                0 => 0.0, // Should not happen (same code caught earlier)
                1 => 1.0,
                2 => 2.0,
                _ => 4.0,
            };
        }

        // Different rings: check relative flip or adjacency
        if a_pos == b_pos {
            // Same number, different letter = relative flip (distance 1)
            return 1.0;
        }

        // Adjacent on the other ring
        let forward = (b_pos as i32 - a_pos as i32).rem_euclid(12);
        let backward = (a_pos as i32 - b_pos as i32).rem_euclid(12);
        let ring_distance = forward.min(backward);

        match ring_distance {
            1 => 2.0, // Adjacent on the other ring
            2 => 2.0, // ±2 on the other ring (maps to distance 2)
            _ => 4.0,
        }
    }

    /// Get the harmonic distance between two Camelot codes.
    pub fn distance(&self, a: CamelotCode, b: CamelotCode) -> f32 {
        self.distances[a.index() as usize][b.index() as usize]
    }
}

impl Default for CamelotTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::newtypes::{Mode, PitchClass};
    use proptest::prelude::*;
    use test_case::test_case;

    /// Helper to build a CamelotCode from number and letter.
    fn camelot(number: u8, letter: char) -> CamelotCode {
        let (target_number, _target_letter) = if letter == 'A' {
            // Minor mode
            match number {
                1 => (8, Mode::Minor),
                2 => (3, Mode::Minor),
                3 => (10, Mode::Minor),
                4 => (5, Mode::Minor),
                5 => (0, Mode::Minor),
                6 => (7, Mode::Minor),
                7 => (2, Mode::Minor),
                8 => (9, Mode::Minor),
                9 => (4, Mode::Minor),
                10 => (11, Mode::Minor),
                11 => (6, Mode::Minor),
                12 => (1, Mode::Minor),
                _ => panic!("Invalid camelot number"),
            }
        } else {
            // Major mode
            match number {
                1 => (11, Mode::Major),
                2 => (6, Mode::Major),
                3 => (1, Mode::Major),
                4 => (8, Mode::Major),
                5 => (3, Mode::Major),
                6 => (10, Mode::Major),
                7 => (5, Mode::Major),
                8 => (0, Mode::Major),
                9 => (7, Mode::Major),
                10 => (2, Mode::Major),
                11 => (9, Mode::Major),
                12 => (4, Mode::Major),
                _ => panic!("Invalid camelot number"),
            }
        };

        let mode = if letter == 'A' {
            Mode::Minor
        } else {
            Mode::Major
        };
        let pitch = PitchClass::new(target_number).expect("valid pitch");
        CamelotCode::from((pitch, mode))
    }

    /// Test identity: distance(c, c) == 0 for all 24 codes.
    #[test]
    fn test_identity() {
        let table = CamelotTable::new();
        for i in 0..24 {
            let code_a = if i < 12 {
                camelot((i as u8 + 1) % 12 + 1, 'B')
            } else {
                camelot((i as u8 - 12) % 12 + 1, 'A')
            };
            assert_eq!(
                table.distance(code_a, code_a),
                0.0,
                "distance({:?}, {:?}) should be 0",
                code_a,
                code_a
            );
        }
    }

    /// Test hand-checked anchors.
    #[test_case('A', 8, 'A', 8, 0.0; "8A to 8A")]
    #[test_case('A', 8, 'A', 9, 1.0; "8A to 9A")]
    #[test_case('A', 8, 'B', 8, 1.0; "8A to 8B")]
    #[test_case('A', 8, 'A', 10, 2.0; "8A to 10A")]
    #[test_case('A', 8, 'B', 9, 2.0; "8A to 9B")]
    #[test_case('A', 8, 'A', 1, 4.0; "8A to 1A")]
    #[test_case('A', 1, 'A', 12, 1.0; "1A to 12A")]
    fn test_hand_checked_anchors(
        a_letter: char,
        a_number: u8,
        b_letter: char,
        b_number: u8,
        expected: f32,
    ) {
        let table = CamelotTable::new();
        let code_a = camelot(a_number, a_letter);
        let code_b = camelot(b_number, b_letter);
        let dist = table.distance(code_a, code_b);
        assert_eq!(
            dist, expected,
            "distance({}{}, {}{}) should be {}, got {}",
            a_number, a_letter, b_number, b_letter, expected, dist
        );
    }

    /// Test coverage: every cell is one of {0.0, 1.0, 2.0, 4.0}.
    #[test]
    fn test_coverage() {
        let table = CamelotTable::new();
        for i in 0..24 {
            for j in 0..24 {
                let dist = table.distances[i][j];
                assert!(
                    [0.0, 1.0, 2.0, 4.0].contains(&dist),
                    "distance at [{}, {}] = {}, not in {{0, 1, 2, 4}}",
                    i,
                    j,
                    dist
                );
            }
        }
    }

    proptest! {
        /// Property test: symmetry for all 24×24 pairs.
        #[test]
        fn prop_symmetry(a_idx in 0usize..24, b_idx in 0usize..24) {
            let table = CamelotTable::new();

            // Convert indices back to CamelotCode
            let code_a = if a_idx < 12 {
                camelot((a_idx as u8 + 1) % 12 + 1, 'B')
            } else {
                camelot((a_idx as u8 - 12) % 12 + 1, 'A')
            };

            let code_b = if b_idx < 12 {
                camelot((b_idx as u8 + 1) % 12 + 1, 'B')
            } else {
                camelot((b_idx as u8 - 12) % 12 + 1, 'A')
            };

            let d_ab = table.distance(code_a, code_b);
            let d_ba = table.distance(code_b, code_a);

            prop_assert_eq!(
                d_ab, d_ba,
                "symmetry violated: d({:?}, {:?}) = {} != {} = d({:?}, {:?})",
                code_a, code_b, d_ab, d_ba, code_b, code_a
            );
        }
    }
}
