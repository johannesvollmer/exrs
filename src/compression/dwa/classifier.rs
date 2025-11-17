//! Channel classification for DWAA/DWAB compression.
//!
//! Determines the compression scheme for each channel based on its name and type.

use crate::meta::attribute::{ChannelList, SampleType};

#[cfg(test)]
use crate::meta::attribute::ChannelDescription;

/// Compression scheme for a channel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionScheme {
    /// Lossy DCT compression (for RGB, Y channels)
    LossyDct,

    /// RLE compression (for alpha channels)
    Rle,

    /// Unknown/fallback (use ZIP)
    Unknown,
}

/// Classification result for a channel
#[derive(Debug, Clone)]
pub struct ChannelClassification {
    /// The compression scheme to use
    pub scheme: CompressionScheme,

    /// Index into the CSC (Color Space Conversion) group, if applicable
    pub csc_group_index: Option<usize>,

    /// Role within the CSC group (0=R, 1=G, 2=B)
    pub csc_channel_role: Option<u8>,
}

/// A group of RGB channels that should be converted to Y'CbCr together
#[derive(Debug, Clone)]
pub struct CscGroup {
    /// Index of the R channel in the channel list
    pub r_index: usize,

    /// Index of the G channel in the channel list
    pub g_index: usize,

    /// Index of the B channel in the channel list
    pub b_index: usize,
}

/// Result of classifying all channels
#[derive(Debug)]
pub struct ClassificationResult {
    /// Classification for each channel
    pub channel_classifications: Vec<ChannelClassification>,

    /// RGB triplets that should use color space conversion
    pub csc_groups: Vec<CscGroup>,
}

/// Classify channels for DWAA/DWAB compression
///
/// # Arguments
/// * `channels` - The channel list to classify
///
/// # Returns
/// Classification results including compression schemes and CSC groups
pub fn classify_channels(channels: &ChannelList) -> ClassificationResult {
    let mut classifications = Vec::new();
    let mut csc_groups = Vec::new();
    let mut used_in_csc = vec![false; channels.list.len()];

    // First pass: identify RGB triplets for CSC
    for i in 0..channels.list.len() {
        if used_in_csc[i] {
            continue;
        }

        let channel = &channels.list[i];

        // Check if this is an R channel (convert Text to String for comparison)
        let channel_name: String = channel.name.clone().into();
        if is_r_channel(&channel_name) && is_float_type(channel.sample_type) {
            // Look for matching G and B channels
            if let Some((g_idx, b_idx)) = find_matching_gb(channels, i, &used_in_csc) {
                // Found a complete RGB triplet
                csc_groups.push(CscGroup {
                    r_index: i,
                    g_index: g_idx,
                    b_index: b_idx,
                });

                used_in_csc[i] = true;
                used_in_csc[g_idx] = true;
                used_in_csc[b_idx] = true;
            }
        }
    }

    // Second pass: classify each channel
    for (i, channel) in channels.list.iter().enumerate() {
        let channel_name: String = channel.name.clone().into();
        let classification = if let Some((group_idx, role)) = find_csc_membership(i, &csc_groups) {
            // Part of an RGB triplet - use lossy DCT with CSC
            ChannelClassification {
                scheme: CompressionScheme::LossyDct,
                csc_group_index: Some(group_idx),
                csc_channel_role: Some(role),
            }
        } else if is_lossy_dct_channel(&channel_name) && is_float_type(channel.sample_type) {
            // Standalone Y, BY, or RY channel - use lossy DCT without CSC
            ChannelClassification {
                scheme: CompressionScheme::LossyDct,
                csc_group_index: None,
                csc_channel_role: None,
            }
        } else if is_alpha_channel(&channel_name) {
            // Alpha channel - use RLE
            ChannelClassification {
                scheme: CompressionScheme::Rle,
                csc_group_index: None,
                csc_channel_role: None,
            }
        } else {
            // Everything else - use ZIP
            ChannelClassification {
                scheme: CompressionScheme::Unknown,
                csc_group_index: None,
                csc_channel_role: None,
            }
        };

        classifications.push(classification);
    }

    ClassificationResult {
        channel_classifications: classifications,
        csc_groups,
    }
}

/// Check if a channel name indicates an R (red) channel
fn is_r_channel(name: &str) -> bool {
    name.ends_with(".R") || name == "R"
}

/// Check if a channel name indicates a G (green) channel
fn is_g_channel(name: &str) -> bool {
    name.ends_with(".G") || name == "G"
}

/// Check if a channel name indicates a B (blue) channel
fn is_b_channel(name: &str) -> bool {
    name.ends_with(".B") || name == "B"
}

/// Check if a channel should use lossy DCT compression
/// (R, G, B, Y, BY, RY channels)
fn is_lossy_dct_channel(name: &str) -> bool {
    name.ends_with(".R") || name == "R" ||
    name.ends_with(".G") || name == "G" ||
    name.ends_with(".B") || name == "B" ||
    name.ends_with(".Y") || name == "Y" ||
    name.ends_with(".BY") || name == "BY" ||
    name.ends_with(".RY") || name == "RY"
}

/// Check if a channel is an alpha channel
fn is_alpha_channel(name: &str) -> bool {
    name.ends_with(".A") || name == "A"
}

/// Check if a sample type is a float type (HALF or FLOAT)
fn is_float_type(sample_type: SampleType) -> bool {
    matches!(sample_type, SampleType::F16 | SampleType::F32)
}

/// Find matching G and B channels for a given R channel
///
/// Returns (g_index, b_index) if found
fn find_matching_gb(
    channels: &ChannelList,
    r_index: usize,
    used: &[bool],
) -> Option<(usize, usize)> {
    let r_name = &channels.list[r_index].name;

    // Derive expected G and B names from R name
    let (g_name, b_name) = if *r_name == *"R" {
        ("G".to_string(), "B".to_string())
    } else {
        // Convert to String to use strip_suffix
        let r_str: String = r_name.clone().into();
        if let Some(prefix) = r_str.strip_suffix(".R") {
            (format!("{}.G", prefix), format!("{}.B", prefix))
        } else {
            return None;
        }
    };

    // Find G channel
    let g_idx = channels
        .list
        .iter()
        .enumerate()
        .position(|(idx, ch)| ch.name == *g_name.as_str() && is_float_type(ch.sample_type) && !used[idx])?;

    // Find B channel
    let b_idx = channels
        .list
        .iter()
        .enumerate()
        .position(|(idx, ch)| ch.name == *b_name.as_str() && is_float_type(ch.sample_type) && !used[idx])?;

    Some((g_idx, b_idx))
}

/// Find the CSC group membership for a channel
///
/// Returns (group_index, role) where role is 0=R, 1=G, 2=B
fn find_csc_membership(channel_idx: usize, groups: &[CscGroup]) -> Option<(usize, u8)> {
    for (group_idx, group) in groups.iter().enumerate() {
        if group.r_index == channel_idx {
            return Some((group_idx, 0));
        } else if group.g_index == channel_idx {
            return Some((group_idx, 1));
        } else if group.b_index == channel_idx {
            return Some((group_idx, 2));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::attribute::Text;

    fn make_channel(name: &str, sample_type: SampleType) -> ChannelDescription {
        ChannelDescription {
            name: Text::from(name),
            sample_type,
            quantize_linearly: false,
            sampling: crate::prelude::Vec2(1, 1),
        }
    }

    fn make_channel_list(channels: Vec<ChannelDescription>) -> ChannelList {
        ChannelList::new(channels.into())
    }

    #[test]
    fn test_classify_rgb_triplet() {
        let channels = make_channel_list(vec![
            make_channel("R", SampleType::F16),
            make_channel("G", SampleType::F16),
            make_channel("B", SampleType::F16),
        ]);

        let result = classify_channels(&channels);

        // Should form one CSC group
        assert_eq!(result.csc_groups.len(), 1);
        assert_eq!(result.csc_groups[0].r_index, 0);
        assert_eq!(result.csc_groups[0].g_index, 1);
        assert_eq!(result.csc_groups[0].b_index, 2);

        // All should use lossy DCT
        for class in &result.channel_classifications {
            assert_eq!(class.scheme, CompressionScheme::LossyDct);
            assert!(class.csc_group_index.is_some());
        }
    }

    #[test]
    fn test_classify_layered_rgb() {
        let channels = make_channel_list(vec![
            make_channel("beauty.R", SampleType::F16),
            make_channel("beauty.G", SampleType::F16),
            make_channel("beauty.B", SampleType::F16),
        ]);

        let result = classify_channels(&channels);

        assert_eq!(result.csc_groups.len(), 1);
        assert_eq!(result.csc_groups[0].r_index, 0);
        assert_eq!(result.csc_groups[0].g_index, 1);
        assert_eq!(result.csc_groups[0].b_index, 2);
    }

    #[test]
    fn test_classify_alpha_channel() {
        let channels = make_channel_list(vec![
            make_channel("A", SampleType::F16),
        ]);

        let result = classify_channels(&channels);

        assert_eq!(result.channel_classifications[0].scheme, CompressionScheme::Rle);
        assert!(result.channel_classifications[0].csc_group_index.is_none());
        assert_eq!(result.csc_groups.len(), 0);
    }

    #[test]
    fn test_classify_y_channel() {
        let channels = make_channel_list(vec![
            make_channel("Y", SampleType::F16),
        ]);

        let result = classify_channels(&channels);

        // Y channel uses lossy DCT without CSC
        assert_eq!(result.channel_classifications[0].scheme, CompressionScheme::LossyDct);
        assert!(result.channel_classifications[0].csc_group_index.is_none());
        assert_eq!(result.csc_groups.len(), 0);
    }

    #[test]
    fn test_classify_mixed_channels() {
        let channels = make_channel_list(vec![
            make_channel("R", SampleType::F16),
            make_channel("G", SampleType::F16),
            make_channel("B", SampleType::F16),
            make_channel("A", SampleType::F16),
            make_channel("Z", SampleType::F32), // Depth - unknown
        ]);

        let result = classify_channels(&channels);

        // RGB should form CSC group
        assert_eq!(result.csc_groups.len(), 1);

        // Check individual classifications
        assert_eq!(result.channel_classifications[0].scheme, CompressionScheme::LossyDct);
        assert_eq!(result.channel_classifications[1].scheme, CompressionScheme::LossyDct);
        assert_eq!(result.channel_classifications[2].scheme, CompressionScheme::LossyDct);
        assert_eq!(result.channel_classifications[3].scheme, CompressionScheme::Rle);
        assert_eq!(result.channel_classifications[4].scheme, CompressionScheme::Unknown);
    }

    #[test]
    fn test_classify_uint_channels() {
        // UINT channels should not use lossy DCT
        let channels = make_channel_list(vec![
            make_channel("R", SampleType::U32),
            make_channel("G", SampleType::U32),
            make_channel("B", SampleType::U32),
        ]);

        let result = classify_channels(&channels);

        // Should not form CSC group (not float types)
        assert_eq!(result.csc_groups.len(), 0);

        // Should all use unknown/ZIP
        for class in &result.channel_classifications {
            assert_eq!(class.scheme, CompressionScheme::Unknown);
        }
    }

    #[test]
    fn test_incomplete_rgb() {
        // Only R and G, no B
        let channels = make_channel_list(vec![
            make_channel("R", SampleType::F16),
            make_channel("G", SampleType::F16),
        ]);

        let result = classify_channels(&channels);

        // Should not form CSC group
        assert_eq!(result.csc_groups.len(), 0);

        // Channels should still use lossy DCT (standalone)
        assert_eq!(result.channel_classifications[0].scheme, CompressionScheme::LossyDct);
        assert_eq!(result.channel_classifications[1].scheme, CompressionScheme::LossyDct);
    }
}
