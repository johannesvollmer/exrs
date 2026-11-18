//! How the LJ2K tests compare exrs's decoded samples with OpenEXR's.
//!
//! OpenEXR's lossy output depends on the CPU: the OpenJPH it links picks
//! AVX2 kernels at run time where it can, and those round some irreversible
//! float conversions differently from its generic code, which the htj2k
//! crate, and so exrs, follow. The installed OpenEXR 3.5.3 links OpenJPH
//! 0.32.0 and uses its AVX2 kernels on the machine the tests were written
//! on, so its decoder gives some samples of the lossy channels one unit in
//! the last place above or below exrs's. So:
//!
//! - a channel that LJ2K codes reversibly (every channel but a lossy R, G and B
//!   triplet) must decode exactly;
//! - a lossy channel may differ by at most one unit in the last place of its
//!   type, with the same sign and both values finite. A NaN, an infinity or a
//!   sign that differs fails.

#![allow(dead_code)]

use exr::meta::attribute::SampleType;

/// A channel as the comparisons need it.
#[derive(Clone, Debug)]
pub struct ChannelInfo {
    pub name: String,
    pub sample_type: SampleType,
    pub sampling: (usize, usize),
}

/// Which channels LJ2K codes lossily: an R, G and B triplet as OpenEXR's
/// `make_channel_map` finds it, of HALF or FLOAT channels of one type and
/// sampling, not subsampled. `channels` are in file order.
pub fn lossy_channels(channels: &[ChannelInfo]) -> Vec<bool> {
    // "r", "g", "b", then "red", "green", "blue": each channel goes to the
    // first triplet whose prefix it shares (any, until the triplet has a
    // channel) and that still lacks its colour
    let names: [[&str; 3]; 2] = [["r", "g", "b"], ["red", "green", "blue"]];
    let mut found: [[Option<usize>; 3]; 2] = [[None; 3]; 2];
    let mut prefixes: [Option<&str>; 2] = [None; 2];
    for (index, channel) in channels.iter().enumerate() {
        let (prefix, suffix) = match channel.name.rfind('.') {
            Some(dot) => (&channel.name[..dot], &channel.name[dot + 1..]),
            None => ("", channel.name.as_str()),
        };
        for triplet in 0..2 {
            if prefixes[triplet].is_some_and(|p| p != prefix) {
                continue;
            }
            let colour = (0..3).find(|&colour| {
                found[triplet][colour].is_none()
                    && suffix.eq_ignore_ascii_case(names[triplet][colour])
            });
            if let Some(colour) = colour {
                found[triplet][colour] = Some(index);
                prefixes[triplet] = Some(prefix);
                break;
            }
        }
    }

    let mut lossy = vec![false; channels.len()];
    for triplet in found {
        if let [Some(r), Some(g), Some(b)] = triplet {
            let first = &channels[r];
            let alike = [g, b].iter().all(|&other| {
                channels[other].sample_type == first.sample_type
                    && channels[other].sampling == first.sampling
            });
            if alike {
                // the first complete triplet is the RGB one
                if first.sample_type != SampleType::U32 && first.sampling == (1, 1) {
                    for index in [r, g, b] {
                        lossy[index] = true;
                    }
                }
                break;
            }
        }
    }
    lossy
}

/// Whether two sample bit patterns of `sample_type` are at most one unit in
/// the last place apart, with the same sign and both finite.
pub fn within_one_ulp(ours: u32, theirs: u32, sample_type: SampleType) -> bool {
    let (sign, exponent) = match sample_type {
        SampleType::F16 => (0x8000, 0x7C00),
        SampleType::F32 => (0x8000_0000, 0x7F80_0000),
        SampleType::U32 => return ours == theirs,
    };
    let finite = |bits: u32| bits & exponent != exponent;
    ours & sign == theirs & sign
        && finite(ours)
        && finite(theirs)
        && (ours & !sign).abs_diff(theirs & !sign) <= 1
}

/// How many units in the last place two sample bit patterns of
/// `sample_type` are apart: the distance between them with the samples
/// ordered as numbers, so that the two zeros are 0 apart (the rules above
/// still fail a change of sign). Bit patterns of UINT
/// samples are their values.
pub fn ulps_apart(ours: u32, theirs: u32, sample_type: SampleType) -> u64 {
    let sign = match sample_type {
        SampleType::F16 => 0x8000,
        SampleType::F32 => 0x8000_0000,
        SampleType::U32 => return u64::from(ours.abs_diff(theirs)),
    };
    let ordered = |bits: u32| {
        let magnitude = i64::from(bits & !sign);
        if bits & sign == 0 {
            magnitude
        } else {
            -magnitude
        }
    };
    ordered(ours).abs_diff(ordered(theirs))
}

/// How the samples of one channel compare.
#[derive(Clone, Debug)]
pub struct ChannelReport {
    pub label: String,
    pub sample_type: SampleType,
    pub lossy: bool,
    pub samples: usize,
    pub differing: usize,
    /// The largest difference, in units in the last place (see
    /// [`ulps_apart`]).
    pub max_ulps: u64,
    /// The first sample that breaks the rules, if any.
    pub violation: Option<String>,
}

impl ChannelReport {
    pub fn share(&self) -> f64 {
        self.differing as f64 / self.samples.max(1) as f64
    }
}

impl std::fmt::Display for ChannelReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} ({:?}, {}): {} of {} samples differ ({:.3} %), by at most {} ULP",
            self.label,
            self.sample_type,
            if self.lossy {
                "lossy"
            } else {
                "reversible"
            },
            self.differing,
            self.samples,
            100.0 * self.share(),
            self.max_ulps
        )?;
        if let Some(violation) = &self.violation {
            write!(formatter, "; FAILS: {violation}")?;
        }
        Ok(())
    }
}

/// Compares the samples of one channel by the rules above.
pub fn compare_channel(
    label: String,
    sample_type: SampleType,
    lossy: bool,
    ours: &[u32],
    theirs: &[u32],
) -> ChannelReport {
    let mut report = ChannelReport {
        label,
        sample_type,
        lossy,
        samples: theirs.len(),
        differing: 0,
        max_ulps: 0,
        violation: None,
    };
    if ours.len() != theirs.len() {
        report.violation = Some(format!("{} samples, OpenEXR's {}", ours.len(), theirs.len()));
        return report;
    }
    for (index, (&ours, &theirs)) in ours.iter().zip(theirs).enumerate() {
        if ours == theirs {
            continue;
        }
        report.differing += 1;
        report.max_ulps = report.max_ulps.max(ulps_apart(ours, theirs, sample_type));
        let allowed = lossy && within_one_ulp(ours, theirs, sample_type);
        if !allowed && report.violation.is_none() {
            report.violation =
                Some(format!("sample {index} is {ours:#x}, OpenEXR decodes {theirs:#x}"));
        }
    }
    report
}

/// Sums reports of one sample type and lossiness: (samples, differing).
pub fn totals<'r>(
    reports: impl IntoIterator<Item = &'r ChannelReport>,
    sample_type: SampleType,
) -> (usize, usize) {
    reports
        .into_iter()
        .filter(|report| report.lossy && report.sample_type == sample_type)
        .fold((0, 0), |(samples, differing), report| {
            (samples + report.samples, differing + report.differing)
        })
}
