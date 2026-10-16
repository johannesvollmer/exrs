//! The `deepImageState` attribute, which describes
//! how the samples inside each pixel of a deep image are organized.

use crate::{
    error::{Error, Result, UnitResult},
    io::{Data, Read, Write},
};

/// Describes how the samples within each pixel of a deep image are organized.
/// This is the value of the standard `deepImageState` attribute.
///
/// Some deep operations, like flattening, require the samples to be sorted
/// and non-overlapping. Knowing that a file already is in that state allows
/// skipping the expensive preparation.
///
/// If the attribute is missing in a file, the samples should be assumed to be
/// [`DeepImageState::Messy`].
///
/// The numerical values match the `DeepImageState` enumeration of the `OpenEXR`
/// library, and are stored as a single byte in the file.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, Default)]
pub enum DeepImageState {
    /// The samples may be in any order, and may overlap each other.
    #[default]
    Messy = 0,

    /// The samples are sorted by their front depth (and then by their back
    /// depth), but volume samples may still overlap.
    Sorted = 1,

    /// No two samples of a pixel overlap each other in depth, but they may
    /// not be sorted.
    NonOverlapping = 2,

    /// The samples are both sorted and non-overlapping.
    Tidy = 3,
}

impl DeepImageState {
    /// The `OpenEXR` type name of this attribute.
    pub const TYPE_NAME: &'static [u8] = super::attribute::type_names::DEEP_IMAGE_STATE;

    /// Whether the samples are sorted by depth.
    #[must_use]
    pub const fn is_sorted(self) -> bool {
        matches!(self, Self::Sorted | Self::Tidy)
    }

    /// Whether no two samples in a pixel overlap each other.
    #[must_use]
    pub const fn is_non_overlapping(self) -> bool {
        matches!(self, Self::NonOverlapping | Self::Tidy)
    }

    /// Whether the samples are both sorted and non-overlapping.
    #[must_use]
    pub const fn is_tidy(self) -> bool {
        matches!(self, Self::Tidy)
    }

    /// Whether this state guarantees at least all the properties
    /// that the `other` state guarantees.
    /// For example, `Tidy` is at least `Sorted`,
    /// but `Sorted` is not at least `NonOverlapping`.
    #[must_use]
    pub const fn is_at_least(self, other: Self) -> bool {
        (self.is_sorted() || !other.is_sorted())
            && (self.is_non_overlapping() || !other.is_non_overlapping())
    }

    /// The numerical value of this state, as it is stored in a file.
    #[must_use]
    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    /// Parse the numerical value of a state, as stored in a file.
    pub fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::Messy),
            1 => Ok(Self::Sorted),
            2 => Ok(Self::NonOverlapping),
            3 => Ok(Self::Tidy),
            _ => Err(Error::invalid("deep image state attribute value")),
        }
    }

    /// Number of bytes this would consume in an exr file.
    #[must_use]
    pub const fn byte_size() -> usize {
        u8::BYTE_SIZE
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(self, write: &mut W) -> UnitResult {
        self.to_u8().write_le(write)
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        Self::from_u8(u8::read_le(read)?)
    }
}

impl std::fmt::Display for DeepImageState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Messy => "messy",
            Self::Sorted => "sorted",
            Self::NonOverlapping => "non-overlapping",
            Self::Tidy => "tidy",
        })
    }
}

#[cfg(test)]
mod test {
    use super::DeepImageState::{self, *};

    const ALL: [DeepImageState; 4] = [Messy, Sorted, NonOverlapping, Tidy];

    #[test]
    fn properties() {
        assert!(!Messy.is_sorted() && !Messy.is_non_overlapping() && !Messy.is_tidy());
        assert!(Sorted.is_sorted() && !Sorted.is_non_overlapping() && !Sorted.is_tidy());
        assert!(!NonOverlapping.is_sorted() && NonOverlapping.is_non_overlapping());
        assert!(Tidy.is_sorted() && Tidy.is_non_overlapping() && Tidy.is_tidy());
        assert_eq!(DeepImageState::default(), Messy);
    }

    #[test]
    fn is_at_least() {
        for state in ALL {
            assert!(state.is_at_least(state));
            assert!(state.is_at_least(Messy));
            assert!(Tidy.is_at_least(state));
        }

        assert!(!Sorted.is_at_least(NonOverlapping));
        assert!(!NonOverlapping.is_at_least(Sorted));
        assert!(!Messy.is_at_least(Sorted));
    }

    #[test]
    fn round_trip_bytes() {
        for state in ALL {
            let mut bytes = Vec::new();
            state.write(&mut bytes).unwrap();
            assert_eq!(bytes.len(), DeepImageState::byte_size());
            assert_eq!(DeepImageState::read(&mut bytes.as_slice()).unwrap(), state);
        }

        assert!(DeepImageState::from_u8(4).is_err());
    }
}
