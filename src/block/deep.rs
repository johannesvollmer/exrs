//! Compression and decompression of deep data blocks.
//!
//! A deep block contains a variable number of samples for each pixel.
//! In the file, each deep chunk contains a pixel offset table and the sample
//! data. The offset table contains, for each pixel, the number of samples in
//! that pixel and all pixels to its left in the same row. Within a block, the
//! sample data is laid out like flat data, except that each pixel may contain
//! any number of samples: for each row of pixels, for each channel, for each
//! pixel in that row, all samples of that pixel.

use std::{convert::TryFrom, ops::Range};

use crate::{
    block::{
        chunk::{
            Chunk, CompressedBlock, CompressedDeepScanLineBlock, CompressedDeepTileBlock,
            TileCoordinates,
        },
        BlockIndex,
    },
    compression::{ByteVec, Compression},
    error::{usize_to_i32, Error, Result, UnitResult},
    math::Vec2,
    meta::{attribute::ChannelList, header::Header, BlockDescription, MetaData},
};

/// Contains a block of deep pixel data, and where that data should be placed
/// in the image.
///
/// Deep pixels contain any number of samples, including zero samples.
/// All channels of a pixel have the same number of samples.
///
/// The pixel offset table is the cumulative sample count of all pixels in this
/// block, in row-major order. Note that this is different from the offset
/// table stored in the file, which restarts counting at each row.
/// The conversion is done when compressing and decompressing.
///
/// The sample data contains the sample values in native-endian format,
/// in the same order as in the file:
/// For each row of pixels in this block, for each channel (sorted
/// alphabetically, as in the channel list of the header), for each pixel in
/// that row, all samples of that pixel. The conversion to little-endian format
/// happens when compressing the block.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct UncompressedDeepBlock {
    /// Location of the data inside the image.
    pub index: BlockIndex,

    /// For each pixel in this block, in row-major order, the total number of
    /// samples of that pixel and all previous pixels in this block.
    /// Contains exactly one entry for each pixel in this block.
    /// The number of samples of pixel `i` is `table[i] - table[i - 1]`,
    /// where the first pixel has `table[0]` samples.
    pub pixel_offset_table: Vec<u32>,

    /// The sample values of all pixels in this block, in native-endian format.
    /// For each row of pixels, for each channel, for each pixel in the row,
    /// contains all samples of that pixel. Contains `total_sample_count *
    /// channels.bytes_per_pixel` bytes.
    pub sample_data: ByteVec,
}

impl UncompressedDeepBlock {
    /// Decompress the possibly compressed deep chunk and return an
    /// `UncompressedDeepBlock`. Returns an error for chunks of flat layers.
    pub fn decompress_chunk(chunk: Chunk, meta_data: &MetaData, pedantic: bool) -> Result<Self> {
        let header: &Header = meta_data
            .headers
            .get(chunk.layer_index)
            .ok_or_else(|| Error::invalid("chunk layer index"))?;

        let tile_data_indices = header.get_block_data_indices(&chunk.compressed_block)?;
        let absolute_indices = header.get_absolute_block_pixel_coordinates(tile_data_indices)?;
        absolute_indices.validate(Some(header.layer_size))?;

        let (compressed_offset_table, compressed_sample_data_le, unpacked_sample_data_size) =
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
                }) => (
                    compressed_pixel_offset_table,
                    compressed_sample_data_le,
                    decompressed_sample_data_size,
                ),

                CompressedBlock::ScanLine(_) | CompressedBlock::Tile(_) => {
                    return Err(Error::invalid(
                        "flat data in deep block (use `UncompressedBlock` for flat data)",
                    ))
                }
            };

        if !header.deep {
            return Err(Error::invalid("deep block in flat layer"));
        }

        let compression = header.compression;
        let block_size = absolute_indices.size;
        let pixel_count = block_size.area();

        let table_byte_size = pixel_count * std::mem::size_of::<i32>();
        let compressed_offset_table: ByteVec =
            compressed_offset_table.into_iter().map(|byte| byte.to_ne_bytes()[0]).collect();

        // the `OpenEXR` library writes uncompressed offset tables of partial
        // tiles using the full tile size, so we allow larger tables and
        // only use the first entries, even when reading pedantically
        let offset_table_le = if compression == Compression::Uncompressed
            || compressed_offset_table.len() == table_byte_size
        {
            compressed_offset_table
        } else {
            let max_table_byte_size = table_byte_size
                .max(header.max_block_pixel_size().area() * std::mem::size_of::<i32>());

            compression.decompress_deep_bytes_le(
                &compressed_offset_table,
                max_table_byte_size,
                pedantic,
            )?
        };

        if offset_table_le.len() < table_byte_size {
            return Err(Error::invalid("deep pixel offset table size"));
        }

        let pixel_offset_table =
            offset_table_from_rows_le(&offset_table_le[..table_byte_size], block_size.width())?;

        let total_sample_count = pixel_offset_table.last().map_or(0, |&total| total as usize);
        let sample_byte_size = total_sample_count
            .checked_mul(header.channels.bytes_per_pixel)
            .ok_or_else(|| Error::invalid("deep sample count"))?;

        if unpacked_sample_data_size < sample_byte_size
            || (pedantic && unpacked_sample_data_size != sample_byte_size)
        {
            return Err(Error::invalid("deep sample data size"));
        }

        let mut sample_data = if compression == Compression::Uncompressed
            || compressed_sample_data_le.len() == unpacked_sample_data_size
        {
            compressed_sample_data_le
        } else {
            compression.decompress_deep_bytes_le(
                &compressed_sample_data_le,
                unpacked_sample_data_size,
                pedantic,
            )?
        };

        if sample_data.len() < sample_byte_size
            || (pedantic && sample_data.len() != unpacked_sample_data_size)
        {
            return Err(Error::invalid("decompressed deep sample data size"));
        }

        sample_data.truncate(sample_byte_size);

        let mut block = Self {
            index: BlockIndex {
                layer: chunk.layer_index,
                pixel_position: absolute_indices.position.to_usize("data indices start")?,
                level: tile_data_indices.level_index,
                pixel_size: block_size,
            },
            pixel_offset_table,
            sample_data,
        };

        if cfg!(target_endian = "big") {
            block.reverse_sample_byte_order(&header.channels)?;
        }

        Ok(block)
    }

    /// Consume this block by compressing it, returning a `Chunk`.
    /// The block must belong to a deep layer.
    pub fn compress_to_chunk(mut self, headers: &[Header]) -> Result<Chunk> {
        let header: &Header =
            headers.get(self.index.layer).ok_or_else(|| Error::invalid("block layer index"))?;

        if !header.deep {
            return Err(Error::invalid("deep block in flat layer"));
        }

        if !header.compression.supports_deep_data() {
            return Err(Error::unsupported(format!(
                "deep data cannot be compressed using {}",
                header.compression
            )));
        }

        self.validate(&header.channels)?;

        let tile_coordinates = TileCoordinates {
            tile_index: self.index.pixel_position / header.max_block_pixel_size(),
            level_index: self.index.level,
        };

        let absolute_indices = header.get_absolute_block_pixel_coordinates(tile_coordinates)?;
        absolute_indices.validate(Some(header.layer_size))?;

        if absolute_indices.position.to_usize("block position")? != self.index.pixel_position
            || absolute_indices.size != self.index.pixel_size
        {
            return Err(Error::invalid("deep block position or size does not match the header"));
        }

        if cfg!(target_endian = "big") {
            self.reverse_sample_byte_order(&header.channels)?;
        }

        let offset_table_le =
            offset_table_to_rows_le(&self.pixel_offset_table, self.index.pixel_size.width())?;

        let decompressed_sample_data_size = self.sample_data.len();
        let compressed_offset_table = header.compression.compress_deep_bytes_le(offset_table_le)?;
        let compressed_sample_data_le =
            header.compression.compress_deep_bytes_le(self.sample_data)?;

        let compressed_pixel_offset_table =
            compressed_offset_table.into_iter().map(|byte| i8::from_ne_bytes([byte])).collect();

        Ok(Chunk {
            layer_index: self.index.layer,
            compressed_block: match header.blocks {
                BlockDescription::ScanLines => {
                    CompressedBlock::DeepScanLine(CompressedDeepScanLineBlock {
                        y_coordinate: usize_to_i32(self.index.pixel_position.y(), "pixel index")?
                            + header.own_attributes.layer_position.y(),
                        decompressed_sample_data_size,
                        compressed_pixel_offset_table,
                        compressed_sample_data_le,
                    })
                }

                BlockDescription::Tiles(_) => CompressedBlock::DeepTile(CompressedDeepTileBlock {
                    coordinates: tile_coordinates,
                    decompressed_sample_data_size,
                    compressed_pixel_offset_table,
                    compressed_sample_data_le,
                }),
            },
        })
    }

    /// The number of pixels in this block.
    #[must_use]
    pub fn pixel_count(&self) -> usize {
        self.pixel_offset_table.len()
    }

    /// The number of samples of all pixels in this block.
    #[must_use]
    pub fn total_sample_count(&self) -> usize {
        self.pixel_offset_table.last().map_or(0, |&total| total as usize)
    }

    /// The number of samples in the specified pixel.
    /// The pixel index is relative to this block, in row-major order.
    /// Panics if the pixel index is out of bounds.
    #[must_use]
    pub fn sample_count(&self, pixel_index: usize) -> usize {
        self.sample_range(pixel_index).len()
    }

    /// The indices of the samples of the specified pixel,
    /// counting all the samples in this block in row-major order.
    /// The pixel index is relative to this block, in row-major order.
    /// Panics if the pixel index is out of bounds.
    #[must_use]
    pub fn sample_range(&self, pixel_index: usize) -> Range<usize> {
        let end = self.pixel_offset_table[pixel_index] as usize;
        let start = if pixel_index == 0 {
            0
        } else {
            self.pixel_offset_table[pixel_index - 1] as usize
        };

        start..end
    }

    /// The range of bytes in `sample_data` that contains all samples
    /// of the specified pixel in the specified channel.
    /// The pixel index is relative to this block, in row-major order.
    /// The channel index refers to the list of channels in the header.
    /// Panics if the pixel or the channel index is out of bounds.
    #[must_use]
    pub fn pixel_sample_bytes_range(
        &self,
        channels: &ChannelList,
        pixel_index: usize,
        channel_index: usize,
    ) -> Range<usize> {
        let width = self.index.pixel_size.width();
        let row = pixel_index / width;
        let row_samples = self.row_sample_range(row);

        let bytes_before_channel: usize = channels.list[..channel_index]
            .iter()
            .map(|channel| channel.sample_type.bytes_per_sample())
            .sum();

        let bytes_per_sample = channels.list[channel_index].sample_type.bytes_per_sample();
        let channel_start =
            row_samples.start * channels.bytes_per_pixel + row_samples.len() * bytes_before_channel;

        let samples = self.sample_range(pixel_index);
        let start = channel_start + (samples.start - row_samples.start) * bytes_per_sample;
        start..start + samples.len() * bytes_per_sample
    }

    /// The indices of the samples in the specified row of pixels in this block,
    /// counting all the samples in this block in row-major order.
    pub(crate) fn row_sample_range(&self, row: usize) -> Range<usize> {
        row_sample_range(&self.pixel_offset_table, self.index.pixel_size.width(), row)
    }

    /// Check that the offset table matches the size of the block,
    /// does not decrease, and matches the size of the sample data.
    pub fn validate(&self, channels: &ChannelList) -> UnitResult {
        if self.index.pixel_size.area() == 0 {
            return Err(Error::invalid("empty deep block"));
        }

        if self.pixel_offset_table.len() != self.index.pixel_size.area() {
            return Err(Error::invalid(format!(
                "deep pixel offset table has {} entries, but the block contains {} pixels",
                self.pixel_offset_table.len(),
                self.index.pixel_size.area()
            )));
        }

        if self.pixel_offset_table.windows(2).any(|pair| pair[1] < pair[0]) {
            return Err(Error::invalid("deep pixel offset table must not decrease"));
        }

        let expected_byte_size = self
            .total_sample_count()
            .checked_mul(channels.bytes_per_pixel)
            .ok_or_else(|| Error::invalid("deep sample count"))?;

        if self.sample_data.len() != expected_byte_size {
            return Err(Error::invalid(format!(
                "deep sample data has {} bytes, but {} samples require {} bytes",
                self.sample_data.len(),
                self.total_sample_count(),
                expected_byte_size
            )));
        }

        Ok(())
    }

    /// Convert the sample data from little-endian to big-endian or vice versa.
    /// Requires a valid offset table.
    fn reverse_sample_byte_order(&mut self, channels: &ChannelList) -> UnitResult {
        let Vec2(width, height) = self.index.pixel_size;
        let mut remaining_bytes = self.sample_data.as_mut_slice();

        for row in 0..height {
            let row_sample_count = row_sample_range(&self.pixel_offset_table, width, row).len();

            for channel in &channels.list {
                let bytes_per_sample = channel.sample_type.bytes_per_sample();
                let byte_count = row_sample_count * bytes_per_sample;

                if remaining_bytes.len() < byte_count {
                    return Err(Error::invalid("deep sample data size"));
                }

                let (channel_bytes, rest) = remaining_bytes.split_at_mut(byte_count);
                for sample_bytes in channel_bytes.chunks_exact_mut(bytes_per_sample) {
                    sample_bytes.reverse();
                }

                remaining_bytes = rest;
            }
        }

        debug_assert!(remaining_bytes.is_empty(), "not all deep samples were converted");
        Ok(())
    }
}

/// The indices of the samples in the specified row of pixels,
/// given an offset table that counts all the samples in a block.
fn row_sample_range(pixel_offset_table: &[u32], width: usize, row: usize) -> Range<usize> {
    let start = if row == 0 {
        0
    } else {
        pixel_offset_table[row * width - 1] as usize
    };
    let end = pixel_offset_table[(row + 1) * width - 1] as usize;
    start..end
}

/// Convert the offset table from the file, which restarts counting in each
/// row, to an offset table that counts all samples in the block.
fn offset_table_from_rows_le(table_le: &[u8], width: usize) -> Result<Vec<u32>> {
    let mut offsets = Vec::with_capacity(table_le.len() / std::mem::size_of::<i32>());
    let mut block_samples_before_row = 0_u32;

    for row_le in table_le.chunks(width.max(1) * std::mem::size_of::<i32>()) {
        let mut previous_row_offset = 0;

        for value_le in row_le.chunks_exact(std::mem::size_of::<i32>()) {
            let row_offset =
                i32::from_le_bytes([value_le[0], value_le[1], value_le[2], value_le[3]]);

            if row_offset < previous_row_offset {
                return Err(Error::invalid("deep pixel offset table must not decrease"));
            }

            previous_row_offset = row_offset;

            let offset = u32::try_from(row_offset)
                .ok()
                .and_then(|row_offset| block_samples_before_row.checked_add(row_offset))
                .ok_or_else(|| Error::invalid("deep block sample count"))?;

            offsets.push(offset);
        }

        block_samples_before_row = offsets.last().copied().unwrap_or(0);
    }

    Ok(offsets)
}

/// Convert an offset table that counts all samples in the block
/// to the offset table that is stored in the file, which restarts counting in
/// each row.
fn offset_table_to_rows_le(offsets: &[u32], width: usize) -> Result<ByteVec> {
    let mut table_le = Vec::with_capacity(offsets.len() * std::mem::size_of::<i32>());
    let mut block_samples_before_row = 0;

    for row in offsets.chunks(width.max(1)) {
        for &offset in row {
            let row_offset = i32::try_from(offset - block_samples_before_row)
                .map_err(|_| Error::invalid("too many deep samples in a single row"))?;

            table_le.extend_from_slice(&row_offset.to_le_bytes());
        }

        block_samples_before_row = row.last().copied().unwrap_or(block_samples_before_row);
    }

    Ok(table_le)
}

#[cfg(test)]
mod test {
    use smallvec::smallvec;

    use super::*;
    use crate::meta::{
        attribute::{ChannelDescription, LevelMode, LineOrder, SampleType, TileDescription},
        Requirements,
    };

    fn test_header(compression: Compression, blocks: BlockDescription) -> Header {
        let channels = smallvec![
            ChannelDescription::new("A", SampleType::F16, true),
            ChannelDescription::new("Z", SampleType::F32, false),
            ChannelDescription::new("id", SampleType::U32, false),
        ];

        let mut header = Header::new("deep".into(), Vec2(5, 3), channels).with_encoding(
            compression,
            blocks,
            LineOrder::Increasing,
        );

        header.deep = true;
        header.deep_data_version = Some(1);
        header.max_samples_per_pixel = Some(4);
        header
    }

    fn meta_data(header: Header) -> MetaData {
        let headers = smallvec![header];
        let requirements = MetaData::validate(&headers, true).unwrap();
        assert_eq!(
            requirements,
            Requirements {
                file_format_version: 2,
                is_single_layer_and_tiled: false,
                has_long_names: false,
                has_deep_data: true,
                has_multiple_layers: false,
            }
        );

        MetaData {
            requirements,
            headers,
        }
    }

    /// Creates a block with deterministic sample values.
    fn test_block(header: &Header, index: BlockIndex, counts: &[u32]) -> UncompressedDeepBlock {
        assert_eq!(counts.len(), index.pixel_size.area());

        let mut pixel_offset_table = Vec::new();
        let mut total = 0;
        for &count in counts {
            total += count;
            pixel_offset_table.push(total);
        }

        let mut block = UncompressedDeepBlock {
            index,
            pixel_offset_table,
            sample_data: vec![0; total as usize * header.channels.bytes_per_pixel],
        };

        let mut value = 0_u32;
        for pixel in 0..counts.len() {
            for (channel_index, channel) in header.channels.list.iter().enumerate() {
                let range = block.pixel_sample_bytes_range(&header.channels, pixel, channel_index);
                let bytes = &mut block.sample_data[range];

                for sample in bytes.chunks_exact_mut(channel.sample_type.bytes_per_sample()) {
                    value += 1;
                    match channel.sample_type {
                        SampleType::F16 => sample.copy_from_slice(
                            &half::f16::from_f32(value as f32 * 0.25).to_ne_bytes(),
                        ),
                        SampleType::F32 => sample.copy_from_slice(&(value as f32).to_ne_bytes()),
                        SampleType::U32 => sample.copy_from_slice(&value.to_ne_bytes()),
                    }
                }
            }
        }

        block
    }

    fn round_trip(compression: Compression, blocks: BlockDescription) {
        let header = test_header(compression, blocks);
        let meta = meta_data(header.clone());

        let counts_per_pixel = [0, 1, 4, 2, 0, 3, 3, 3, 0, 1, 1, 2, 4, 0, 0];

        for (block_number, (_, index)) in
            crate::block::enumerate_ordered_header_block_indices(&meta.headers).enumerate()
        {
            let counts: Vec<u32> = (0..index.pixel_size.area())
                .map(|pixel| counts_per_pixel[(pixel + block_number) % counts_per_pixel.len()])
                .collect();

            let block = test_block(&header, index, &counts);
            block.validate(&header.channels).unwrap();

            let chunk = block.clone().compress_to_chunk(&meta.headers).unwrap();
            let decompressed = UncompressedDeepBlock::decompress_chunk(chunk, &meta, true).unwrap();
            assert_eq!(decompressed, block, "{compression:?}, {blocks:?}");
        }
    }

    #[test]
    fn round_trip_all_compression_methods_and_block_types() {
        let tiles = BlockDescription::Tiles(TileDescription {
            tile_size: Vec2(2, 2),
            level_mode: LevelMode::Singular,
            rounding_mode: crate::math::RoundingMode::Down,
        });

        for compression in [Compression::Uncompressed, Compression::RLE, Compression::ZIP1] {
            round_trip(compression, BlockDescription::ScanLines);
            round_trip(compression, tiles);
        }
    }

    #[test]
    fn offset_table_restarts_in_each_row() {
        // two rows of three pixels with the sample counts [1, 0, 2] and [3, 1,
        // 0]
        let offsets = vec![1, 1, 3, 6, 7, 7];

        let table_le = offset_table_to_rows_le(&offsets, 3).unwrap();
        let table: Vec<i32> = table_le
            .chunks_exact(4)
            .map(|bytes| i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect();

        // this is how the `OpenEXR` library stores the table
        assert_eq!(table, vec![1, 1, 3, 3, 4, 4]);
        assert_eq!(offset_table_from_rows_le(&table_le, 3).unwrap(), offsets);
    }

    #[test]
    fn reject_decreasing_offset_table() {
        let table_le: Vec<u8> = [2_i32, 1].iter().flat_map(|value| value.to_le_bytes()).collect();
        assert!(offset_table_from_rows_le(&table_le, 2).is_err());

        let table_le: Vec<u8> = [2_i32, -1].iter().flat_map(|value| value.to_le_bytes()).collect();
        assert!(offset_table_from_rows_le(&table_le, 1).is_err());
    }

    #[test]
    fn sample_bytes_are_ordered_by_row_then_channel_then_pixel() {
        let header = test_header(Compression::Uncompressed, BlockDescription::ScanLines);
        let channels = &header.channels; // A: f16, Z: f32, id: u32 = 10 bytes
                                         // per sample

        let block = UncompressedDeepBlock {
            index: BlockIndex {
                layer: 0,
                pixel_position: Vec2(0, 0),
                pixel_size: Vec2(2, 2),
                level: Vec2(0, 0),
            },
            pixel_offset_table: vec![1, 3, 3, 4],
            sample_data: vec![0; 40],
        };

        block.validate(channels).unwrap();

        // first row: A of both pixels, Z of both pixels, id of both pixels
        assert_eq!(block.pixel_sample_bytes_range(channels, 0, 0), 0..2);
        assert_eq!(block.pixel_sample_bytes_range(channels, 1, 0), 2..6);
        assert_eq!(block.pixel_sample_bytes_range(channels, 0, 1), 6..10);
        assert_eq!(block.pixel_sample_bytes_range(channels, 1, 1), 10..18);
        assert_eq!(block.pixel_sample_bytes_range(channels, 1, 2), 22..30);

        // second row: the first pixel has no samples
        assert_eq!(block.pixel_sample_bytes_range(channels, 2, 0), 30..30);
        assert_eq!(block.pixel_sample_bytes_range(channels, 3, 0), 30..32);
        assert_eq!(block.pixel_sample_bytes_range(channels, 3, 1), 32..36);
        assert_eq!(block.pixel_sample_bytes_range(channels, 3, 2), 36..40);
    }

    #[test]
    fn reverse_byte_order_twice_is_identity() {
        let header = test_header(Compression::Uncompressed, BlockDescription::ScanLines);
        let index = BlockIndex {
            layer: 0,
            pixel_position: Vec2(0, 0),
            pixel_size: Vec2(5, 1),
            level: Vec2(0, 0),
        };

        let original = test_block(&header, index, &[1, 0, 3, 2, 1]);
        let mut block = original.clone();

        block.reverse_sample_byte_order(&header.channels).unwrap();
        assert_ne!(block, original);

        // the first channel is `A` of type f16
        let first = original.pixel_sample_bytes_range(&header.channels, 0, 0);
        assert_eq!(block.sample_data[first.start], original.sample_data[first.start + 1]);

        block.reverse_sample_byte_order(&header.channels).unwrap();
        assert_eq!(block, original);
    }

    #[test]
    fn reject_invalid_blocks() {
        let header = test_header(Compression::RLE, BlockDescription::ScanLines);
        let meta = meta_data(header.clone());
        let index = BlockIndex {
            layer: 0,
            pixel_position: Vec2(0, 0),
            pixel_size: Vec2(5, 1),
            level: Vec2(0, 0),
        };

        let mut block = test_block(&header, index, &[1, 0, 3, 2, 1]);
        block.sample_data.pop();
        assert!(block.compress_to_chunk(&meta.headers).is_err());

        let mut block = test_block(&header, index, &[1, 0, 3, 2, 1]);
        block.pixel_offset_table.pop();
        assert!(block.compress_to_chunk(&meta.headers).is_err());

        let mut block = test_block(&header, index, &[1, 0, 3, 2, 1]);
        block.index.pixel_size = Vec2(4, 1);
        assert!(block.compress_to_chunk(&meta.headers).is_err());
    }
}
