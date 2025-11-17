//! This is the low-level interface for the raw blocks of an image.
//! See `exr::image` module for a high-level interface.
//!
//! Handle compressed and uncompressed pixel byte blocks. Includes compression and decompression,
//! and reading a complete image into blocks.
//!
//! Start with the `block::read(...)`
//! and `block::write(...)` functions.

pub mod reader;
pub mod writer;

pub mod chunk;
pub mod lines;
pub mod samples;

use crate::block::chunk::{
    Chunk, CompressedBlock, CompressedScanLineBlock, CompressedTileBlock, TileCoordinates,
};
use crate::block::lines::{LineIndex, LineRef, LineRefMut, LineSlice};
use crate::compression::ByteVec;
use crate::error::{usize_to_i32, Error, Result, UnitResult};
use crate::math::Vec2;
use crate::meta::attribute::ChannelList;
use crate::meta::header::Header;
use crate::meta::{BlockDescription, Headers, MetaData};
use std::io::{Read, Seek, Write};

/// Specifies where a block of pixel data should be placed in the actual image.
/// This is a globally unique identifier which
/// includes the layer, level index, and pixel location.
#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
pub struct BlockIndex {
    /// Index of the layer.
    pub layer: usize,

    /// Index of the top left pixel from the block within the data window.
    pub pixel_position: Vec2<usize>,

    /// Number of pixels in this block, extending to the right and downwards.
    /// Stays the same across all resolution levels.
    pub pixel_size: Vec2<usize>,

    /// Index of the mip or rip level in the image.
    pub level: Vec2<usize>,
}

/// Contains a block of pixel data and where that data should be placed in the actual image.
///
/// The bytes must be encoded in native-endian format.
/// The conversion to little-endian format happens when converting to chunks (potentially in parallel).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct UncompressedBlock {
    /// Location of the data inside the image.
    pub index: BlockIndex,

    /// Uncompressed pixel values of the whole block.
    /// One or more scan lines may be stored together as a scan line block.
    /// This byte vector contains all pixel rows, one after another.
    /// For each line in the tile, for each channel, the row values are contiguous.
    /// Stores all samples of the first channel, then all samples of the second channel, and so on.
    /// This data is in native-endian format.
    pub data: ByteVec,
}

/// Contains a block of deep pixel data and where that data should be placed in the actual image.
///
/// Deep images store multiple samples per pixel at different depths. This block contains:
/// - A pixel offset table: cumulative sample counts for efficient random access.
/// - Sample data: all samples for all pixels, organized pixel-by-pixel.
///
/// The bytes must be encoded in native-endian format.
#[cfg(feature = "deep")]
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct UncompressedDeepBlock {
    /// Location of the data inside the image.
    pub index: BlockIndex,

    /// Cumulative sample counts per pixel.
    /// For a block with N pixels, this has N entries.
    /// Entry i contains the total number of samples in pixels 0..=i.
    /// To get the sample count for pixel i:
    /// - If i == 0: count = `pixel_offset_table[0]`
    /// - Else: count = `pixel_offset_table[i] - pixel_offset_table[i-1]`
    pub pixel_offset_table: Vec<i32>,

    /// Sample data in native-endian format.
    /// Layout (matching the file format): for each channel, for each pixel (scanline order),
    /// for each sample: value. The total number of samples is pixel_offset_table[N-1].
    pub sample_data: ByteVec,
}

/// Immediately reads the meta data from the file.
///
/// Then, returns a reader that can be used to read all pixel blocks.
/// From the reader, you can pull each compressed chunk from the file.
/// Alternatively, you can create a decompressor, and pull the uncompressed data from it.
/// The reader is assumed to be buffered.
pub fn read<R: Read + Seek>(buffered_read: R, pedantic: bool) -> Result<self::reader::Reader<R>> {
    self::reader::Reader::read_from_buffered(buffered_read, pedantic)
}

/// Immediately writes the meta data to the file.
///
/// Then, calls a closure with a writer that can be used to write all pixel blocks.
/// In the closure, you can push compressed chunks directly into the writer.
/// Alternatively, you can create a compressor, wrapping the writer, and push the uncompressed data to it.
/// The writer is assumed to be buffered.
pub fn write<W: Write + Seek>(
    buffered_write: W,
    headers: Headers,
    compatibility_checks: bool,
    write_chunks: impl FnOnce(MetaData, &mut self::writer::ChunkWriter<W>) -> UnitResult,
) -> UnitResult {
    self::writer::write_chunks_with(buffered_write, headers, compatibility_checks, write_chunks)
}

/// This iterator tells you the block indices of all blocks that must be in the image.
///
/// The order of the blocks depends on the `LineOrder` attribute
/// (unspecified line order is treated the same as increasing line order).
/// The blocks written to the file must be exactly in this order,
/// except for when the `LineOrder` is unspecified.
/// The index represents the block index, in increasing line order, within the header.
pub fn enumerate_ordered_header_block_indices(
    headers: &[Header],
) -> impl '_ + Iterator<Item = (usize, BlockIndex)> {
    headers
        .iter()
        .enumerate()
        .flat_map(|(layer_index, header)| {
            header
                .enumerate_ordered_blocks()
                .map(move |(index_in_header, tile)| {
                    let data_indices = header
                        .absolute_block_pixel_coordinates(tile.location)
                        .expect("tile coordinate bug");

                    let block = BlockIndex {
                        layer: layer_index,
                        level: tile.location.level_index,
                        pixel_position: data_indices
                            .position
                            .to_usize("data indices start")
                            .expect("data index bug"),
                        pixel_size: data_indices.size,
                    };

                    (index_in_header, block)
                })
        })
}

impl UncompressedBlock {
    /// Decompress the possibly compressed chunk and returns an `UncompressedBlock`.
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    #[inline]
    #[must_use]
    pub fn decompress_chunk(chunk: Chunk, meta_data: &MetaData, pedantic: bool) -> Result<Self> {
        let header: &Header = meta_data
            .headers
            .get(chunk.layer_index)
            .ok_or_else(|| Error::invalid("chunk layer index"))?;

        let tile_data_indices = header.block_data_indices(&chunk.compressed_block)?;
        let absolute_indices = header.absolute_block_pixel_coordinates(tile_data_indices)?;

        absolute_indices.validate(Some(header.layer_size))?;

        match chunk.compressed_block {
            CompressedBlock::Tile(CompressedTileBlock {
                compressed_pixels_le,
                ..
            })
            | CompressedBlock::ScanLine(CompressedScanLineBlock {
                compressed_pixels_le,
                ..
            }) => Ok(Self {
                data: header.compression.decompress_image_section_from_le(
                    header,
                    compressed_pixels_le,
                    absolute_indices,
                    pedantic,
                )?,
                index: BlockIndex {
                    layer: chunk.layer_index,
                    pixel_position: absolute_indices.position.to_usize("data indices start")?,
                    level: tile_data_indices.level_index,
                    pixel_size: absolute_indices.size,
                },
            }),

            #[cfg(not(feature = "deep"))]
            _ => Err(Error::unsupported(
                "deep data support is not enabled; enable the 'deep' feature"
            )),

            #[cfg(feature = "deep")]
            CompressedBlock::DeepScanLine(_) | CompressedBlock::DeepTile(_) => {
                Err(Error::unsupported(
                    "use UncompressedDeepBlock::decompress_chunk for deep data"
                ))
            }
        }
    }

    /// Consume this block by compressing it, returning a `Chunk`.
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    #[inline]
    #[must_use]
    pub fn compress_to_chunk(self, headers: &[Header]) -> Result<Chunk> {
        let Self { data, index } = self;

        let header: &Header = headers.get(index.layer).expect("block layer index bug");

        let expected_byte_size = header.channels.bytes_per_pixel * self.index.pixel_size.area(); // TODO sampling??
        if expected_byte_size != data.len() {
            return Err(Error::invalid(format!(
                "decompressed block byte size mismatch: expected {} bytes but got {} bytes",
                expected_byte_size,
                data.len()
            )));
        }

        let tile_coordinates = TileCoordinates {
            // FIXME this calculation should not be made here but elsewhere instead (in meta::header?)
            tile_index: index.pixel_position / header.max_block_pixel_size(), // TODO sampling??
            level_index: index.level,
        };

        let absolute_indices = header.absolute_block_pixel_coordinates(tile_coordinates)?;
        absolute_indices.validate(Some(header.layer_size))?;

        if !header.compression.may_loose_data() {
            debug_assert_eq!(
                &header
                    .compression
                    .decompress_image_section_from_le(
                        header,
                        header.compression.compress_image_section_to_le(
                            header,
                            data.clone(),
                            absolute_indices
                        )?,
                        absolute_indices,
                        true
                    )
                    .unwrap(),
                &data,
                "compression method not round trippin'"
            );
        }

        let compressed_pixels_le =
            header
                .compression
                .compress_image_section_to_le(header, data, absolute_indices)?;

        Ok(Chunk {
            layer_index: index.layer,
            compressed_block: match header.blocks {
                BlockDescription::ScanLines => CompressedBlock::ScanLine(CompressedScanLineBlock {
                    compressed_pixels_le,

                    // FIXME this calculation should not be made here but elsewhere instead (in meta::header?)
                    y_coordinate: usize_to_i32(index.pixel_position.y(), "pixel index")?
                        + header.own_attributes.layer_position.y(), // TODO sampling??
                }),

                BlockDescription::Tiles(_) => CompressedBlock::Tile(CompressedTileBlock {
                    compressed_pixels_le,
                    coordinates: tile_coordinates,
                }),
            },
        })
    }

    /// Iterate all the lines in this block.
    /// Each line contains the all samples for one of the channels.
    pub fn lines(&self, channels: &ChannelList) -> impl Iterator<Item = LineRef<'_>> {
        LineIndex::lines_in_block(self.index, channels).map(move |(bytes, line)| LineSlice {
            location: line,
            value: &self.data[bytes],
        })
    }

    /* TODO pub fn lines_mut<'s>(&'s mut self, header: &Header) -> impl 's + Iterator<Item=LineRefMut<'s>> {
        LineIndex::lines_in_block(self.index, &header.channels)
            .map(move |(bytes, line)| LineSlice { location: line, value: &mut self.data[bytes] })
    }*/

    /*// TODO make iterator
    /// Call a closure for each line of samples in this uncompressed block.
    pub fn for_lines(
        &self, header: &Header,
        mut accept_line: impl FnMut(LineRef<'_>) -> UnitResult
    ) -> UnitResult {
        for (bytes, line) in LineIndex::lines_in_block(self.index, &header.channels) {
            let line_ref = LineSlice { location: line, value: &self.data[bytes] };
            accept_line(line_ref)?;
        }

        Ok(())
    }*/

    // TODO from iterator??
    /// Create an uncompressed block byte vector by requesting one line of samples after another.
    pub fn collect_block_data_from_lines(
        channels: &ChannelList,
        block_index: BlockIndex,
        mut extract_line: impl FnMut(LineRefMut<'_>),
    ) -> Vec<u8> {
        let byte_count = block_index.pixel_size.area() * channels.bytes_per_pixel;
        let mut block_bytes = vec![0_u8; byte_count];

        for (byte_range, line_index) in LineIndex::lines_in_block(block_index, channels) {
            extract_line(LineRefMut {
                // TODO subsampling
                value: &mut block_bytes[byte_range],
                location: line_index,
            });
        }

        block_bytes
    }

    /// Create an uncompressed block by requesting one line of samples after another.
    pub fn from_lines(
        channels: &ChannelList,
        block_index: BlockIndex,
        extract_line: impl FnMut(LineRefMut<'_>),
    ) -> Self {
        Self {
            index: block_index,
            data: Self::collect_block_data_from_lines(channels, block_index, extract_line),
        }
    }
}

#[cfg(feature = "deep")]
impl UncompressedDeepBlock {
    /// Decompress a deep data chunk and return an `UncompressedDeepBlock`.
    ///
    /// Deep data blocks contain:
    /// - A pixel offset table: cumulative sample counts for each pixel
    /// - Sample data: all samples for all pixels, organized pixel-by-pixel
    #[inline]
    #[must_use]
    pub fn decompress_chunk(chunk: Chunk, meta_data: &MetaData, _pedantic: bool) -> Result<Self> {
        use crate::block::chunk::{CompressedDeepScanLineBlock, CompressedDeepTileBlock};

        let header: &Header = meta_data
            .headers
            .get(chunk.layer_index)
            .ok_or_else(|| Error::invalid("chunk layer index"))?;

        let tile_data_indices = header.block_data_indices(&chunk.compressed_block)?;
        let absolute_indices = header.absolute_block_pixel_coordinates(tile_data_indices)?;

        absolute_indices.validate(Some(header.layer_size))?;

        // Verify the compression method supports deep data
        if !header.compression.supports_deep_data() {
            return Err(Error::unsupported(format!(
                "compression method {:?} does not support deep data",
                header.compression
            )));
        }

        match chunk.compressed_block {
            CompressedBlock::DeepScanLine(CompressedDeepScanLineBlock {
                compressed_pixel_offset_table,
                compressed_sample_data_le,
                decompressed_sample_data_size,
                ..
            })
            | CompressedBlock::DeepTile(CompressedDeepTileBlock {
                compressed_pixel_offset_table,
                compressed_sample_data_le,
                decompressed_sample_data_size,
                ..
            }) => {
                let num_pixels = absolute_indices.size.area();

                // Decompress the pixel offset table
                let pixel_offset_table = header.compression.decompress_deep_offset_table(
                    &compressed_pixel_offset_table,
                    num_pixels,
                )?;

                // Decompress the sample data
                let sample_data = header.compression.decompress_deep_sample_data(
                    header,
                    compressed_sample_data_le,
                    decompressed_sample_data_size,
                )?;

                Ok(Self {
                    index: BlockIndex {
                        layer: chunk.layer_index,
                        pixel_position: absolute_indices.position.to_usize("data indices start")?,
                        level: tile_data_indices.level_index,
                        pixel_size: absolute_indices.size,
                    },
                    pixel_offset_table,
                    sample_data,
                })
            }

            _ => Err(Error::invalid(
                "expected deep scanline or deep tile block for deep data"
            )),
        }
    }

    /// Consume this deep block by compressing it, returning a `Chunk`.
    #[inline]
    #[must_use]
    pub fn compress_to_chunk(self, headers: &[Header]) -> Result<Chunk> {
        use crate::block::chunk::{CompressedDeepScanLineBlock, CompressedDeepTileBlock};

        let Self {
            pixel_offset_table,
            sample_data,
            index,
        } = self;

        let header: &Header = headers.get(index.layer).expect("block layer index bug");

        // Verify the compression method supports deep data
        if !header.compression.supports_deep_data() {
            return Err(Error::unsupported(format!(
                "compression method {:?} does not support deep data",
                header.compression
            )));
        }

        let num_pixels = index.pixel_size.area();

        // Verify offset table size
        if pixel_offset_table.len() != num_pixels {
            return Err(Error::invalid(format!(
                "pixel offset table size mismatch: expected {} entries, got {}",
                num_pixels,
                pixel_offset_table.len()
            )));
        }

        let tile_coordinates = TileCoordinates {
            tile_index: index.pixel_position / header.max_block_pixel_size(),
            level_index: index.level,
        };

        let absolute_indices = header.absolute_block_pixel_coordinates(tile_coordinates)?;
        absolute_indices.validate(Some(header.layer_size))?;

        // Compress the pixel offset table and sample data
        let decompressed_sample_data_size = sample_data.len();
        let (compressed_pixel_offset_table, compressed_sample_data_le) = header
            .compression
            .compress_deep_block(header, pixel_offset_table, sample_data)?;

        Ok(Chunk {
            layer_index: index.layer,
            compressed_block: match header.blocks {
                BlockDescription::ScanLines => {
                    CompressedBlock::DeepScanLine(CompressedDeepScanLineBlock {
                        y_coordinate: usize_to_i32(index.pixel_position.y(), "pixel index")?
                            + header.own_attributes.layer_position.y(),
                        decompressed_sample_data_size,
                        compressed_pixel_offset_table,
                        compressed_sample_data_le,
                    })
                }

                BlockDescription::Tiles(_) => {
                    CompressedBlock::DeepTile(CompressedDeepTileBlock {
                        coordinates: tile_coordinates,
                        decompressed_sample_data_size,
                        compressed_pixel_offset_table,
                        compressed_sample_data_le,
                    })
                }
            },
        })
    }
}

#[cfg(all(test, feature = "deep"))]
mod deep_tests {
    use super::*;
    use crate::meta::{BlockDescription, Requirements};
    use crate::compression::Compression;
    use crate::meta::attribute::{ChannelDescription, SampleType};
    use crate::meta::header::Header;
    use smallvec::smallvec;

    #[test]
    fn test_deep_block_round_trip_uncompressed() {
        test_deep_block_round_trip(Compression::Uncompressed);
    }

    #[test]
    fn test_deep_block_round_trip_rle() {
        test_deep_block_round_trip(Compression::RLE);
    }

    #[test]
    fn test_deep_block_round_trip_zip1() {
        test_deep_block_round_trip(Compression::ZIP1);
    }

    #[test]
    fn test_deep_block_round_trip_zip16() {
        test_deep_block_round_trip(Compression::ZIP16);
    }

    fn test_deep_block_round_trip(compression: Compression) {
        // Create a simple test header with deep data
        let channels = smallvec![
            ChannelDescription {
                name: "Z".into(),
                sample_type: SampleType::F32,
                quantize_linearly: false,
                sampling: Vec2(1, 1),
            },
            ChannelDescription {
                name: "ZBack".into(),
                sample_type: SampleType::F32,
                quantize_linearly: false,
                sampling: Vec2(1, 1),
            },
        ];

        let mut header = Header::new("deep_test".into(), Vec2(4, 4), channels);
        header.compression = compression;
        header.blocks = BlockDescription::ScanLines;
        header.deep = true;
        header.deep_data_version = Some(1);
        header.max_samples_per_pixel = Some(3);

        let headers = smallvec![header];

        // Create test data: 4x4 block with varying sample counts
        let block_size = Vec2(4, 4);
        let num_pixels = block_size.area();

        // Sample counts: [1, 2, 1, 0, 3, 1, 1, 2, 0, 1, 2, 1, 1, 1, 0, 1]
        let sample_counts = vec![1, 2, 1, 0, 3, 1, 1, 2, 0, 1, 2, 1, 1, 1, 0, 1];

        // Convert to cumulative offsets
        let mut pixel_offset_table = Vec::with_capacity(num_pixels);
        let mut cumulative = 0i32;
        for &count in &sample_counts {
            cumulative += count;
            pixel_offset_table.push(cumulative);
        }

        let total_samples = cumulative as usize;

        // Create sample data: each sample has 2 channels (Z and ZBack), each F32 (4 bytes)
        let bytes_per_sample = 8; // 2 channels * 4 bytes
        let mut sample_data = vec![0u8; total_samples * bytes_per_sample];

        // Fill with test pattern
        for i in 0..total_samples * 2 {
            let value = (i as f32 + 0.5).to_ne_bytes();
            let offset = i * 4;
            sample_data[offset..offset + 4].copy_from_slice(&value);
        }

        // Create the uncompressed deep block
        let original_block = UncompressedDeepBlock {
            index: BlockIndex {
                layer: 0,
                pixel_position: Vec2(0, 0),
                level: Vec2(0, 0),
                pixel_size: block_size,
            },
            pixel_offset_table: pixel_offset_table.clone(),
            sample_data: sample_data.clone(),
        };

        // Compress to chunk
        let chunk = original_block
            .compress_to_chunk(&headers)
            .expect("compression should succeed");

        // Create metadata for decompression
        let meta_data = MetaData {
            requirements: Requirements {
                file_format_version: 2,
                is_single_layer_and_tiled: false,
                has_long_names: false,
                has_deep_data: true,
                has_multiple_layers: false,
            },
            headers,
        };

        // Decompress back
        let decompressed_block = UncompressedDeepBlock::decompress_chunk(chunk, &meta_data, true)
            .expect("decompression should succeed");

        // Verify the data matches
        assert_eq!(
            decompressed_block.pixel_offset_table, pixel_offset_table,
            "pixel offset table should match after round-trip"
        );
        assert_eq!(
            decompressed_block.sample_data, sample_data,
            "sample data should match after round-trip"
        );
        assert_eq!(
            decompressed_block.index.pixel_size, block_size,
            "block size should match"
        );
    }
}
