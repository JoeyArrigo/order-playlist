//! Domain newtypes with validation baked into constructors.
//!
//! `Bpm` and `PitchClass` use fallible `new()` constructors that return
//! `Result<Self, DomainError>`. `Normalized` uses a forgiving `clamp()`
//! constructor that emits a `tracing::warn!` on out-of-range input and a
//! strict `try_new()` for callers who would rather see the error.

use serde::{Deserialize, Serialize};

/// Unified error type for domain validation failures.
#[derive(Debug, thiserror::Error, miette::Diagnostic, PartialEq)]
pub enum DomainError {
    /// BPM must be finite and > 0.
    #[error("Bpm must be finite and > 0, got {0}")]
    InvalidBpm(f32),

    /// PitchClass must be 0..=11.
    #[error("PitchClass must be 0..=11, got {0}")]
    InvalidPitchClass(u8),

    /// Normalized value must be finite and in [0.0, 1.0].
    #[error("Normalized value must be finite and in [0.0, 1.0], got {0}")]
    InvalidNormalized(f32),
}

/// Beats per minute. Constructor rejects non-finite values, zero, and negatives.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bpm(f32);

impl Bpm {
    /// Construct a `Bpm` from a value, returning an error if the value is
    /// non-finite, zero, or negative.
    ///
    /// # Examples
    ///
    /// ```
    /// # use playlistize::domain::Bpm;
    /// assert!(Bpm::new(120.5).is_ok());
    /// assert!(Bpm::new(0.0).is_err());
    /// assert!(Bpm::new(f32::NAN).is_err());
    /// ```
    pub fn new(value: f32) -> Result<Bpm, DomainError> {
        if !value.is_finite() || value <= 0.0 {
            Err(DomainError::InvalidBpm(value))
        } else {
            Ok(Bpm(value))
        }
    }

    /// Access the underlying BPM value.
    pub fn get(&self) -> f32 {
        self.0
    }

    /// The default fallback BPM (120). Pre-validated — bypasses `new()`.
    pub const DEFAULT_120: Bpm = Bpm(120.0);
}

/// Pitch class (0..=11 representing C, C#, D, ... B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PitchClass(u8);

impl PitchClass {
    /// Construct a `PitchClass` from a value, returning an error if ≥ 12.
    ///
    /// # Examples
    ///
    /// ```
    /// # use playlistize::domain::PitchClass;
    /// assert!(PitchClass::new(0).is_ok());   // C
    /// assert!(PitchClass::new(11).is_ok());  // B
    /// assert!(PitchClass::new(12).is_err());
    /// ```
    pub fn new(value: u8) -> Result<PitchClass, DomainError> {
        if value >= 12 {
            Err(DomainError::InvalidPitchClass(value))
        } else {
            Ok(PitchClass(value))
        }
    }

    /// Access the underlying pitch class value (0..=11).
    pub fn get(&self) -> u8 {
        self.0
    }

    /// C (pitch class 0). Pre-validated — bypasses `new()`.
    pub const C: PitchClass = PitchClass(0);
}

/// Musical mode: major or minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Major mode.
    Major,
    /// Minor mode.
    Minor,
}

impl Default for Mode {
    /// Default mode is Major. Used as fallback when adapters omit the field.
    fn default() -> Self {
        Mode::Major
    }
}

/// Normalized value in [0.0, 1.0]. Provides both forgiving `clamp()` and
/// strict `try_new()` constructors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Normalized(f32);

impl Normalized {
    /// Clamp a value to [0.0, 1.0], emitting a `tracing::warn!` if the input
    /// was out-of-range or non-finite.
    ///
    /// This constructor is idempotent: `clamp(clamp(x)) == clamp(x)`.
    ///
    /// Non-finite values (`NaN`, infinity) are clamped to 0.0 with a warning.
    ///
    /// # Examples
    ///
    /// ```
    /// # use playlistize::domain::Normalized;
    /// assert_eq!(Normalized::clamp(0.5).get(), 0.5);
    /// assert_eq!(Normalized::clamp(-1.0).get(), 0.0);  // warns
    /// assert_eq!(Normalized::clamp(2.0).get(), 1.0);   // warns
    /// assert_eq!(Normalized::clamp(f32::NAN).get(), 0.0); // warns
    /// ```
    pub fn clamp(value: f32) -> Normalized {
        if !value.is_finite() {
            tracing::warn!("Normalized::clamp received non-finite value: {}", value);
            Normalized(0.0)
        } else if value < 0.0 {
            tracing::warn!("Normalized::clamp received value below 0.0: {}", value);
            Normalized(0.0)
        } else if value > 1.0 {
            tracing::warn!("Normalized::clamp received value above 1.0: {}", value);
            Normalized(1.0)
        } else {
            Normalized(value)
        }
    }

    /// Strictly construct a `Normalized` from a value, returning an error if
    /// the value is non-finite or out of range.
    ///
    /// # Examples
    ///
    /// ```
    /// # use playlistize::domain::Normalized;
    /// assert!(Normalized::try_new(0.5).is_ok());
    /// assert!(Normalized::try_new(-0.001).is_err());
    /// assert!(Normalized::try_new(1.001).is_err());
    /// ```
    pub fn try_new(value: f32) -> Result<Normalized, DomainError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            Err(DomainError::InvalidNormalized(value))
        } else {
            Ok(Normalized(value))
        }
    }

    /// Access the underlying normalized value.
    pub fn get(&self) -> f32 {
        self.0
    }

    /// 0.0 — pre-validated, bypasses `clamp()`.
    pub const ZERO: Normalized = Normalized(0.0);

    /// 0.5 — pre-validated, bypasses `clamp()`.
    pub const HALF: Normalized = Normalized(0.5);

    /// 1.0 — pre-validated, bypasses `clamp()`.
    pub const ONE: Normalized = Normalized(1.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    // ======================
    // Bpm tests
    // ======================

    #[test]
    fn bpm_rejects_nan() {
        let result = Bpm::new(f32::NAN);
        assert!(matches!(result, Err(DomainError::InvalidBpm(_))));
    }

    #[test]
    fn bpm_rejects_positive_infinity() {
        assert_eq!(
            Bpm::new(f32::INFINITY),
            Err(DomainError::InvalidBpm(f32::INFINITY))
        );
    }

    #[test]
    fn bpm_rejects_negative_infinity() {
        assert_eq!(
            Bpm::new(f32::NEG_INFINITY),
            Err(DomainError::InvalidBpm(f32::NEG_INFINITY))
        );
    }

    #[test]
    fn bpm_rejects_zero() {
        assert_eq!(Bpm::new(0.0), Err(DomainError::InvalidBpm(0.0)));
    }

    #[test]
    fn bpm_rejects_negative() {
        assert_eq!(Bpm::new(-1.0), Err(DomainError::InvalidBpm(-1.0)));
    }

    #[test]
    fn bpm_accepts_60() {
        assert!(Bpm::new(60.0).is_ok());
    }

    #[test]
    fn bpm_accepts_120_5() {
        let bpm = Bpm::new(120.5).expect("120.5 is valid");
        assert_eq!(bpm.get(), 120.5);
    }

    #[test]
    fn bpm_accepts_200() {
        assert!(Bpm::new(200.0).is_ok());
    }

    // ======================
    // PitchClass tests
    // ======================

    #[test_case(0; "pitch_class_0")]
    #[test_case(1; "pitch_class_1")]
    #[test_case(2; "pitch_class_2")]
    #[test_case(3; "pitch_class_3")]
    #[test_case(4; "pitch_class_4")]
    #[test_case(5; "pitch_class_5")]
    #[test_case(6; "pitch_class_6")]
    #[test_case(7; "pitch_class_7")]
    #[test_case(8; "pitch_class_8")]
    #[test_case(9; "pitch_class_9")]
    #[test_case(10; "pitch_class_10")]
    #[test_case(11; "pitch_class_11")]
    fn pitch_class_accepts_0_to_11(value: u8) {
        let pc = PitchClass::new(value)
            .unwrap_or_else(|_| panic!("PitchClass {} should be valid", value));
        assert_eq!(pc.get(), value);
    }

    #[test]
    fn pitch_class_rejects_12() {
        assert_eq!(PitchClass::new(12), Err(DomainError::InvalidPitchClass(12)));
    }

    #[test]
    fn pitch_class_rejects_100() {
        assert_eq!(
            PitchClass::new(100),
            Err(DomainError::InvalidPitchClass(100))
        );
    }

    #[test]
    fn pitch_class_rejects_255() {
        assert_eq!(
            PitchClass::new(255),
            Err(DomainError::InvalidPitchClass(255))
        );
    }

    // ======================
    // Mode tests (no validation needed)
    // ======================

    #[test]
    fn mode_default_is_major() {
        assert_eq!(Mode::default(), Mode::Major);
    }

    // ======================
    // Normalized tests
    // ======================

    #[test]
    fn normalized_try_new_accepts_valid() {
        let n = Normalized::try_new(0.5).expect("0.5 is valid");
        assert_eq!(n.get(), 0.5);
    }

    #[test]
    fn normalized_try_new_rejects_nan() {
        let result = Normalized::try_new(f32::NAN);
        assert!(matches!(result, Err(DomainError::InvalidNormalized(_))));
    }

    #[test]
    fn normalized_try_new_rejects_negative() {
        assert_eq!(
            Normalized::try_new(-0.001),
            Err(DomainError::InvalidNormalized(-0.001))
        );
    }

    #[test]
    fn normalized_try_new_rejects_above_one() {
        assert_eq!(
            Normalized::try_new(1.001),
            Err(DomainError::InvalidNormalized(1.001))
        );
    }

    #[test]
    fn normalized_try_new_rejects_infinity() {
        assert_eq!(
            Normalized::try_new(f32::INFINITY),
            Err(DomainError::InvalidNormalized(f32::INFINITY))
        );
    }

    #[test]
    fn normalized_clamp_valid_value() {
        let n = Normalized::clamp(0.5);
        assert_eq!(n.get(), 0.5);
    }

    #[test]
    fn normalized_clamp_nan_to_zero() {
        let n = Normalized::clamp(f32::NAN);
        assert_eq!(n.get(), 0.0);
    }

    #[test]
    fn normalized_clamp_negative_to_zero() {
        let n = Normalized::clamp(-1.5);
        assert_eq!(n.get(), 0.0);
    }

    #[test]
    fn normalized_clamp_above_one() {
        let n = Normalized::clamp(2.0);
        assert_eq!(n.get(), 1.0);
    }

    #[test]
    fn normalized_clamp_idempotence() {
        // For any value, clamp(clamp(x)).get() == clamp(x).get()
        let values = vec![
            -5.0,
            -1.0,
            0.0,
            0.25,
            0.5,
            0.75,
            1.0,
            1.5,
            2.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];

        for &val in &values {
            let once = Normalized::clamp(val);
            let twice = Normalized::clamp(once.get());
            assert_eq!(
                once.get(),
                twice.get(),
                "idempotence failed for value {}",
                val
            );
        }
    }

    #[test]
    fn normalized_constants_are_correct() {
        assert_eq!(Normalized::ZERO.get(), 0.0);
        assert_eq!(Normalized::HALF.get(), 0.5);
        assert_eq!(Normalized::ONE.get(), 1.0);
    }

    // ======================
    // Property tests using proptest
    // ======================

    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn normalized_clamp_idempotence_property(value in any::<f32>()) {
                let once = Normalized::clamp(value);
                let twice = Normalized::clamp(once.get());
                prop_assert_eq!(once.get(), twice.get(), "idempotence failed for {}", value);
            }

            #[test]
            fn normalized_clamp_result_in_range(value in any::<f32>()) {
                let n = Normalized::clamp(value);
                let clamped = n.get();
                prop_assert!(
                    (0.0..=1.0).contains(&clamped),
                    "clamped value {} not in [0.0, 1.0]",
                    clamped
                );
            }
        }
    }
}
