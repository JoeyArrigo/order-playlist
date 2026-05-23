//! Camelot wheel mapping for harmonic mixing.
//!
//! Maps musical key + mode to a 24-position notation used by DJs for harmonic
//! mixing. Adjacent positions on the wheel are harmonically compatible.
//!
//! The Camelot system uses two rings:
//! - B ring (major mode): positions 1B through 12B
//! - A ring (minor mode): positions 1A through 12A
//!
//! Each musical key (pitch class 0..=11) combined with a mode (Major/Minor)
//! maps to a unique Camelot code on the wheel.

use crate::domain::newtypes::{Mode, PitchClass};
use serde::{Deserialize, Serialize};

/// Letter designation on the Camelot wheel: A (minor) or B (major).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CamelotLetter {
    /// A ring (minor mode).
    A,
    /// B ring (major mode).
    B,
}

/// A Camelot code: a number (1..=12) and a letter (A or B).
///
/// The Camelot wheel maps all 24 musical keys (12 pitch classes × 2 modes)
/// to unique positions. Adjacent positions on the wheel are harmonically compatible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CamelotCode {
    /// Camelot number: 1 through 12.
    pub number: u8,
    /// Camelot letter: A (minor) or B (major).
    pub letter: CamelotLetter,
}

impl CamelotCode {
    /// Returns a 0..=23 index for use as a row/column in lookup tables.
    ///
    /// **Convention (load-bearing — used by Phase 3's CamelotTable):**
    /// - B ring (major): index = number - 1 → 0..=11
    ///   (1B → 0, 2B → 1, ..., 12B → 11)
    /// - A ring (minor): index = 11 + number → 12..=23
    ///   (1A → 12, 2A → 13, ..., 12A → 23)
    pub fn index(&self) -> u8 {
        match self.letter {
            CamelotLetter::B => self.number - 1,
            CamelotLetter::A => 11 + self.number,
        }
    }
}

impl From<(PitchClass, Mode)> for CamelotCode {
    /// Maps a pitch class and mode to its Camelot code using the standard
    /// 12-tone equal temperament Camelot mapping.
    ///
    /// The mapping is derived from the conventional Camelot wheel used by
    /// Mixed In Key, Beatport, Rekordbox, and other DJ software.
    fn from((pc, mode): (PitchClass, Mode)) -> Self {
        let pitch_value = pc.get();
        match mode {
            Mode::Major => {
                // Major mode → B ring
                let number = match pitch_value {
                    0 => 8,  // C  → 8B
                    1 => 3,  // C# → 3B
                    2 => 10, // D  → 10B
                    3 => 5,  // D# → 5B
                    4 => 12, // E  → 12B
                    5 => 7,  // F  → 7B
                    6 => 2,  // F# → 2B
                    7 => 9,  // G  → 9B
                    8 => 4,  // G# → 4B
                    9 => 11, // A  → 11B
                    10 => 6, // A# → 6B
                    11 => 1, // B  → 1B
                    _ => unreachable!("PitchClass is validated to 0..=11"),
                };
                CamelotCode {
                    number,
                    letter: CamelotLetter::B,
                }
            }
            Mode::Minor => {
                // Minor mode → A ring
                let number = match pitch_value {
                    0 => 5,   // C  → 5A
                    1 => 12,  // C# → 12A
                    2 => 7,   // D  → 7A
                    3 => 2,   // D# → 2A
                    4 => 9,   // E  → 9A
                    5 => 4,   // F  → 4A
                    6 => 11,  // F# → 11A
                    7 => 6,   // G  → 6A
                    8 => 1,   // G# → 1A
                    9 => 8,   // A  → 8A
                    10 => 3,  // A# → 3A
                    11 => 10, // B  → 10A
                    _ => unreachable!("PitchClass is validated to 0..=11"),
                };
                CamelotCode {
                    number,
                    letter: CamelotLetter::A,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    // ============================================================================
    // Mapping tests: 24 parameterized cases for (PitchClass, Mode) → CamelotCode
    // ============================================================================

    #[test_case(0, Mode::Major, 8, CamelotLetter::B; "C Major -> 8B")]
    #[test_case(1, Mode::Major, 3, CamelotLetter::B; "C# Major -> 3B")]
    #[test_case(2, Mode::Major, 10, CamelotLetter::B; "D Major -> 10B")]
    #[test_case(3, Mode::Major, 5, CamelotLetter::B; "D# Major -> 5B")]
    #[test_case(4, Mode::Major, 12, CamelotLetter::B; "E Major -> 12B")]
    #[test_case(5, Mode::Major, 7, CamelotLetter::B; "F Major -> 7B")]
    #[test_case(6, Mode::Major, 2, CamelotLetter::B; "F# Major -> 2B")]
    #[test_case(7, Mode::Major, 9, CamelotLetter::B; "G Major -> 9B")]
    #[test_case(8, Mode::Major, 4, CamelotLetter::B; "G# Major -> 4B")]
    #[test_case(9, Mode::Major, 11, CamelotLetter::B; "A Major -> 11B")]
    #[test_case(10, Mode::Major, 6, CamelotLetter::B; "A# Major -> 6B")]
    #[test_case(11, Mode::Major, 1, CamelotLetter::B; "B Major -> 1B")]
    #[test_case(0, Mode::Minor, 5, CamelotLetter::A; "C Minor -> 5A")]
    #[test_case(1, Mode::Minor, 12, CamelotLetter::A; "C# Minor -> 12A")]
    #[test_case(2, Mode::Minor, 7, CamelotLetter::A; "D Minor -> 7A")]
    #[test_case(3, Mode::Minor, 2, CamelotLetter::A; "D# Minor -> 2A")]
    #[test_case(4, Mode::Minor, 9, CamelotLetter::A; "E Minor -> 9A")]
    #[test_case(5, Mode::Minor, 4, CamelotLetter::A; "F Minor -> 4A")]
    #[test_case(6, Mode::Minor, 11, CamelotLetter::A; "F# Minor -> 11A")]
    #[test_case(7, Mode::Minor, 6, CamelotLetter::A; "G Minor -> 6A")]
    #[test_case(8, Mode::Minor, 1, CamelotLetter::A; "G# Minor -> 1A")]
    #[test_case(9, Mode::Minor, 8, CamelotLetter::A; "A Minor -> 8A")]
    #[test_case(10, Mode::Minor, 3, CamelotLetter::A; "A# Minor -> 3A")]
    #[test_case(11, Mode::Minor, 10, CamelotLetter::A; "B Minor -> 10A")]
    fn test_camelot_mapping(
        pitch_value: u8,
        mode: Mode,
        expected_number: u8,
        expected_letter: CamelotLetter,
    ) {
        let pc = PitchClass::new(pitch_value).expect("pitch_value is 0..=11");
        let code = CamelotCode::from((pc, mode));

        assert_eq!(
            code.number, expected_number,
            "Camelot number mismatch for pitch {}, mode {:?}",
            pitch_value, mode
        );
        assert_eq!(
            code.letter, expected_letter,
            "Camelot letter mismatch for pitch {}, mode {:?}",
            pitch_value, mode
        );
    }

    // ============================================================================
    // Bijection test: all 24 Camelot codes have unique indices in 0..=23
    // ============================================================================

    #[test]
    fn test_index_bijection() {
        let mut indices = [false; 24];

        for pitch_value in 0..=11 {
            let pc = PitchClass::new(pitch_value).expect("valid pitch");

            for mode in [Mode::Major, Mode::Minor] {
                let code = CamelotCode::from((pc, mode));
                let idx = code.index();

                assert!(
                    idx < 24,
                    "Index out of range: pitch {}, mode {:?}, index {}",
                    pitch_value,
                    mode,
                    idx
                );

                assert!(
                    !indices[idx as usize],
                    "Duplicate index {} for pitch {}, mode {:?}",
                    idx, pitch_value, mode
                );

                indices[idx as usize] = true;
            }
        }

        // Verify all 24 indices are used
        for (i, &used) in indices.iter().enumerate() {
            assert!(used, "Index {} is unused", i);
        }
    }

    // ============================================================================
    // Round-trip test: (pc, mode) → CamelotCode → index → reverse-lookup
    // ============================================================================

    /// Reverse lookup: given a CamelotCode, recover the original (PitchClass, Mode).
    /// This is used for round-trip testing only.
    fn reverse_lookup(code: CamelotCode) -> (PitchClass, Mode) {
        match code.letter {
            CamelotLetter::B => {
                // Major mode
                let pitch_value = match code.number {
                    1 => 11, // 1B  → B
                    2 => 6,  // 2B  → F#
                    3 => 1,  // 3B  → C#
                    4 => 8,  // 4B  → G#
                    5 => 3,  // 5B  → D#
                    6 => 10, // 6B  → A#
                    7 => 5,  // 7B  → F
                    8 => 0,  // 8B  → C
                    9 => 7,  // 9B  → G
                    10 => 2, // 10B → D
                    11 => 9, // 11B → A
                    12 => 4, // 12B → E
                    _ => panic!("Invalid Camelot number: {}", code.number),
                };
                (
                    PitchClass::new(pitch_value).expect("valid pitch"),
                    Mode::Major,
                )
            }
            CamelotLetter::A => {
                // Minor mode
                let pitch_value = match code.number {
                    1 => 8,   // 1A  → G#
                    2 => 3,   // 2A  → D#
                    3 => 10,  // 3A  → A#
                    4 => 5,   // 4A  → F
                    5 => 0,   // 5A  → C
                    6 => 7,   // 6A  → G
                    7 => 2,   // 7A  → D
                    8 => 9,   // 8A  → A
                    9 => 4,   // 9A  → E
                    10 => 11, // 10A → B
                    11 => 6,  // 11A → F#
                    12 => 1,  // 12A → C#
                    _ => panic!("Invalid Camelot number: {}", code.number),
                };
                (
                    PitchClass::new(pitch_value).expect("valid pitch"),
                    Mode::Minor,
                )
            }
        }
    }

    #[test]
    fn test_round_trip() {
        for pitch_value in 0..=11 {
            let pc = PitchClass::new(pitch_value).expect("valid pitch");

            for mode in [Mode::Major, Mode::Minor] {
                let original = (pc, mode);
                let code = CamelotCode::from(original);
                let recovered = reverse_lookup(code);

                assert_eq!(
                    original.0.get(),
                    recovered.0.get(),
                    "Pitch class mismatch in round-trip for pitch {}, mode {:?}",
                    pitch_value,
                    mode
                );

                assert_eq!(
                    original.1, recovered.1,
                    "Mode mismatch in round-trip for pitch {}, mode {:?}",
                    pitch_value, mode
                );
            }
        }
    }
}
