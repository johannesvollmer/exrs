//! Deep image state attribute for tracking sample organization.
//!
//! This module defines the `DeepImageState` enum which describes how deep samples
//! are organized within a deep image. This affects what operations can be performed
//! efficiently on the image.

#[cfg(feature = "deep-data")]
use crate::error::{Result, Error, UnitResult};

/// Describes the organization state of samples in a deep image.
///
/// Deep images store multiple samples per pixel at different depths. The organization
/// of these samples affects what operations can be performed efficiently:
///
/// - **Messy**: Samples may be in any order and may overlap. This is the most flexible
///   state but requires tidying before many operations.
///
/// - **Sorted**: Samples are sorted by depth (Z value) but may still overlap. This
///   enables efficient depth-based operations but not merging.
///
/// - **NonOverlapping**: Samples don't overlap each other but may not be sorted. This
///   enables efficient merging but not depth-based traversal.
///
/// - **Tidy**: Samples are both sorted and non-overlapping. This is the optimal state
///   for most operations including flattening and compositing.
///
/// # OpenEXR Specification
///
/// This corresponds to the `DeepImageState` attribute in the OpenEXR specification.
/// The integer values match the C++ implementation exactly.
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "deep-data")]
/// # {
/// use exr::meta::deep_state::DeepImageState;
///
/// // A newly created deep image is typically messy
/// let state = DeepImageState::Messy;
///
/// // After sorting samples by depth
/// let state = DeepImageState::Sorted;
///
/// // After making non-overlapping (may require splitting volume samples)
/// let state = DeepImageState::NonOverlapping;
///
/// // After both sorting and making non-overlapping
/// let state = DeepImageState::Tidy;
/// # }
/// ```
#[cfg(feature = "deep-data")]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(u8)]
pub enum DeepImageState {
    /// Samples are neither sorted nor non-overlapping.
    /// This is the most flexible state but requires processing before many operations.
    Messy = 0,

    /// Samples are sorted by depth (Z value) but may overlap.
    /// Volume samples (Z < ZBack) may overlap with other samples.
    Sorted = 1,

    /// Samples are non-overlapping but not necessarily sorted.
    /// No two samples in a pixel overlap in depth.
    NonOverlapping = 2,

    /// Samples are both sorted by depth and non-overlapping.
    /// This is the optimal state for compositing and flattening operations.
    Tidy = 3,
}

#[cfg(feature = "deep-data")]
impl DeepImageState {
    /// Returns `true` if samples are sorted by depth.
    #[inline]
    pub fn is_sorted(self) -> bool {
        matches!(self, DeepImageState::Sorted | DeepImageState::Tidy)
    }

    /// Returns `true` if samples are non-overlapping.
    #[inline]
    pub fn is_non_overlapping(self) -> bool {
        matches!(self, DeepImageState::NonOverlapping | DeepImageState::Tidy)
    }

    /// Returns `true` if samples are both sorted and non-overlapping.
    #[inline]
    pub fn is_tidy(self) -> bool {
        self == DeepImageState::Tidy
    }

    /// Returns `true` if this state is at least as organized as `other`.
    ///
    /// Tidy is the most organized state, messy is the least.
    pub fn is_at_least(self, other: DeepImageState) -> bool {
        use DeepImageState::*;

        match (self, other) {
            (Tidy, _) => true,
            (_, Tidy) => false,
            (Sorted, Sorted) | (Sorted, Messy) => true,
            (NonOverlapping, NonOverlapping) | (NonOverlapping, Messy) => true,
            (Messy, Messy) => true,
            _ => false,
        }
    }

    /// Converts the state to its integer representation.
    ///
    /// This matches the OpenEXR C++ implementation exactly.
    #[inline]
    pub fn to_i32(self) -> i32 {
        self as u8 as i32
    }

    /// Creates a `DeepImageState` from its integer representation.
    ///
    /// Returns an error if the value is not a valid state (0-3).
    pub fn from_i32(value: i32) -> Result<Self> {
        match value {
            0 => Ok(DeepImageState::Messy),
            1 => Ok(DeepImageState::Sorted),
            2 => Ok(DeepImageState::NonOverlapping),
            3 => Ok(DeepImageState::Tidy),
            _ => Err(Error::invalid(format!(
                "invalid DeepImageState value: {} (must be 0-3)",
                value
            ))),
        }
    }

    /// Validates that this state is appropriate for the given operation.
    ///
    /// Some operations require samples to be in a specific state:
    /// - Flattening requires `Tidy`
    /// - Compositing typically requires `Sorted`
    pub fn require_for_operation(self, operation: &str, required: DeepImageState) -> UnitResult {
        if !self.is_at_least(required) {
            Err(Error::invalid(format!(
                "operation '{}' requires deep image state {:?}, but image is {:?}",
                operation, required, self
            )))
        } else {
            Ok(())
        }
    }
}

#[cfg(feature = "deep-data")]
impl Default for DeepImageState {
    /// Returns `Messy` as the default state for newly created deep images.
    fn default() -> Self {
        DeepImageState::Messy
    }
}

#[cfg(feature = "deep-data")]
impl std::fmt::Display for DeepImageState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeepImageState::Messy => write!(f, "messy"),
            DeepImageState::Sorted => write!(f, "sorted"),
            DeepImageState::NonOverlapping => write!(f, "non-overlapping"),
            DeepImageState::Tidy => write!(f, "tidy"),
        }
    }
}

#[cfg(all(test, feature = "deep-data"))]
mod tests {
    use super::*;

    #[test]
    fn test_state_properties() {
        assert!(!DeepImageState::Messy.is_sorted());
        assert!(!DeepImageState::Messy.is_non_overlapping());
        assert!(!DeepImageState::Messy.is_tidy());

        assert!(DeepImageState::Sorted.is_sorted());
        assert!(!DeepImageState::Sorted.is_non_overlapping());
        assert!(!DeepImageState::Sorted.is_tidy());

        assert!(!DeepImageState::NonOverlapping.is_sorted());
        assert!(DeepImageState::NonOverlapping.is_non_overlapping());
        assert!(!DeepImageState::NonOverlapping.is_tidy());

        assert!(DeepImageState::Tidy.is_sorted());
        assert!(DeepImageState::Tidy.is_non_overlapping());
        assert!(DeepImageState::Tidy.is_tidy());
    }

    #[test]
    fn test_is_at_least() {
        assert!(DeepImageState::Tidy.is_at_least(DeepImageState::Messy));
        assert!(DeepImageState::Tidy.is_at_least(DeepImageState::Sorted));
        assert!(DeepImageState::Tidy.is_at_least(DeepImageState::NonOverlapping));
        assert!(DeepImageState::Tidy.is_at_least(DeepImageState::Tidy));

        assert!(DeepImageState::Sorted.is_at_least(DeepImageState::Messy));
        assert!(DeepImageState::Sorted.is_at_least(DeepImageState::Sorted));
        assert!(!DeepImageState::Sorted.is_at_least(DeepImageState::NonOverlapping));
        assert!(!DeepImageState::Sorted.is_at_least(DeepImageState::Tidy));

        assert!(DeepImageState::NonOverlapping.is_at_least(DeepImageState::Messy));
        assert!(!DeepImageState::NonOverlapping.is_at_least(DeepImageState::Sorted));
        assert!(DeepImageState::NonOverlapping.is_at_least(DeepImageState::NonOverlapping));
        assert!(!DeepImageState::NonOverlapping.is_at_least(DeepImageState::Tidy));

        assert!(DeepImageState::Messy.is_at_least(DeepImageState::Messy));
        assert!(!DeepImageState::Messy.is_at_least(DeepImageState::Sorted));
        assert!(!DeepImageState::Messy.is_at_least(DeepImageState::NonOverlapping));
        assert!(!DeepImageState::Messy.is_at_least(DeepImageState::Tidy));
    }

    #[test]
    fn test_conversion() {
        // Test to_i32
        assert_eq!(DeepImageState::Messy.to_i32(), 0);
        assert_eq!(DeepImageState::Sorted.to_i32(), 1);
        assert_eq!(DeepImageState::NonOverlapping.to_i32(), 2);
        assert_eq!(DeepImageState::Tidy.to_i32(), 3);

        // Test from_i32
        assert_eq!(DeepImageState::from_i32(0).unwrap(), DeepImageState::Messy);
        assert_eq!(DeepImageState::from_i32(1).unwrap(), DeepImageState::Sorted);
        assert_eq!(DeepImageState::from_i32(2).unwrap(), DeepImageState::NonOverlapping);
        assert_eq!(DeepImageState::from_i32(3).unwrap(), DeepImageState::Tidy);

        // Test invalid values
        assert!(DeepImageState::from_i32(-1).is_err());
        assert!(DeepImageState::from_i32(4).is_err());
        assert!(DeepImageState::from_i32(100).is_err());
    }

    #[test]
    fn test_round_trip() {
        for &state in &[
            DeepImageState::Messy,
            DeepImageState::Sorted,
            DeepImageState::NonOverlapping,
            DeepImageState::Tidy,
        ] {
            let value = state.to_i32();
            let restored = DeepImageState::from_i32(value).unwrap();
            assert_eq!(state, restored);
        }
    }

    #[test]
    fn test_default() {
        assert_eq!(DeepImageState::default(), DeepImageState::Messy);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", DeepImageState::Messy), "messy");
        assert_eq!(format!("{}", DeepImageState::Sorted), "sorted");
        assert_eq!(format!("{}", DeepImageState::NonOverlapping), "non-overlapping");
        assert_eq!(format!("{}", DeepImageState::Tidy), "tidy");
    }

    #[test]
    fn test_require_for_operation() {
        // Tidy state allows any operation
        assert!(DeepImageState::Tidy
            .require_for_operation("flatten", DeepImageState::Tidy)
            .is_ok());
        assert!(DeepImageState::Tidy
            .require_for_operation("composite", DeepImageState::Sorted)
            .is_ok());

        // Messy state fails for operations requiring sorted/tidy
        assert!(DeepImageState::Messy
            .require_for_operation("flatten", DeepImageState::Tidy)
            .is_err());
        assert!(DeepImageState::Messy
            .require_for_operation("composite", DeepImageState::Sorted)
            .is_err());

        // Sorted is OK for sorted operations but not tidy operations
        assert!(DeepImageState::Sorted
            .require_for_operation("composite", DeepImageState::Sorted)
            .is_ok());
        assert!(DeepImageState::Sorted
            .require_for_operation("flatten", DeepImageState::Tidy)
            .is_err());
    }
}
