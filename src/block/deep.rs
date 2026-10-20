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
#[derive(Clone, Eq, PartialEq, Debug, Hash)]
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

        let table_byte_size = block_size
            .width()
            .checked_mul(block_size.height())
            .and_then(|pixel_count| pixel_count.checked_mul(std::mem::size_of::<i32>()))
            .ok_or_else(|| Error::invalid("deep block size"))?;
        let compressed_offset_table: ByteVec =
            compressed_offset_table.into_iter().map(|byte| byte.to_ne_bytes()[0]).collect();

        let full_table_byte_size = full_block_table_byte_size(header).unwrap_or(usize::MAX);
        let is_partial_tile = header.blocks.has_tiles() && table_byte_size < full_table_byte_size;

        // The `OpenEXR` library writes uncompressed offset tables of partial
        // tiles padded to the full tile size, so we allow larger tables and
        // only use the first entries, even when reading pedantically.
        // Older versions also store tables of partial tiles in compressed
        // layers this way, when compression does not make them smaller than
        // the table of a full tile.
        let is_uncompressed_table = compression == Compression::Uncompressed
            || compressed_offset_table.len() == table_byte_size
            || (is_partial_tile && compressed_offset_table.len() == full_table_byte_size);

        let read_offset_table = |is_uncompressed_table: bool| -> Result<Vec<u32>> {
            let decompressed_table_le;
            let offset_table_le = if is_uncompressed_table {
                &compressed_offset_table
            } else {
                decompressed_table_le = compression.decompress_deep_bytes_le(
                    &compressed_offset_table,
                    table_byte_size.max(full_table_byte_size),
                    pedantic,
                )?;

                &decompressed_table_le
            };

            let offset_table_le = offset_table_le
                .get(..table_byte_size)
                .ok_or_else(|| Error::invalid("deep pixel offset table size"))?;

            offset_table_from_rows_le(offset_table_le, block_size.width())
        };

        let sample_byte_size_of = |pixel_offset_table: &[u32]| -> Result<usize> {
            let total_sample_count = pixel_offset_table.last().map_or(0, |&total| total as usize);
            total_sample_count
                .checked_mul(header.channels.bytes_per_pixel)
                .ok_or_else(|| Error::invalid("deep sample count"))
        };

        let mut pixel_offset_table = read_offset_table(is_uncompressed_table);

        // Older versions of the `OpenEXR` library store a compressed table for
        // a partial tile, even if it has exactly the size of the uncompressed
        // table, as they compare the size to the table of a full tile.
        // We only recognize such a table by trying to decompress it,
        // when it is not a valid uncompressed table.
        let is_ambiguous = is_partial_tile
            && compression != Compression::Uncompressed
            && compressed_offset_table.len() == table_byte_size;

        let matches_sample_data = |table: &Result<Vec<u32>>| match table {
            Ok(table) => sample_byte_size_of(table).ok() == Some(unpacked_sample_data_size),
            Err(_) => false,
        };

        if is_ambiguous && !matches_sample_data(&pixel_offset_table) {
            let decompressed_table = read_offset_table(false);

            if matches_sample_data(&decompressed_table) {
                pixel_offset_table = decompressed_table;
            }
        }

        let pixel_offset_table = pixel_offset_table?;
        let sample_byte_size = sample_byte_size_of(&pixel_offset_table)?;

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
        let compressed_offset_table = compress_offset_table(header, offset_table_le)?;
        let compressed_sample_data_le =
            compress_unless_larger(header.compression, self.sample_data)?;

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
    pub fn pixel_sample_byte_range(
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

/// The byte size of the uncompressed pixel offset table of a block
/// that is not cut off by the edge of the image.
/// Returns `None` if the size does not fit into `usize`.
fn full_block_table_byte_size(header: &Header) -> Option<usize> {
    let Vec2(width, height) = header.max_block_pixel_size();
    width.checked_mul(height)?.checked_mul(std::mem::size_of::<i32>())
}

/// Compress the bytes, unless that would not reduce their size,
/// just like the `OpenEXR` library does.
fn compress_unless_larger(compression: Compression, bytes_le: ByteVec) -> Result<ByteVec> {
    if compression == Compression::Uncompressed {
        return Ok(bytes_le);
    }

    let compressed_le = compression.compress_deep_bytes_le(&bytes_le)?;
    Ok(if compressed_le.len() < bytes_le.len() {
        compressed_le
    } else {
        bytes_le
    })
}

/// Compress the pixel offset table of a block,
/// such that all versions of the `OpenEXR` library can read it in most cases.
///
/// Readers decide whether a table is stored without compression by comparing
/// its size to the uncompressed size. `OpenEXR` 3.3 and newer compare it to
/// the uncompressed table of this block. `OpenEXR` 3.2 and older compare it to
/// the uncompressed table of a full tile, which is larger for the partial
/// tiles at the right and bottom edge of an image.
///
/// Like the `OpenEXR` writer, we store uncompressed tables of partial tiles
/// padded to the full tile size, unless the table of a full tile has more than
/// 16 MiB, which `OpenEXR` 3.2 and older then cannot read. For partial tiles
/// of compressed layers, we store a compressed table whose size differs from
/// both sizes. If we find no such compressed table, we store the table without
/// compression or padding, like newer versions of the `OpenEXR` writer do,
/// which `OpenEXR` 3.2 and older cannot read. This can happen for partial tiles
/// of tiles that are only one or two pixels wide or high, and for partial
/// tiles that are only one or two pixels smaller than a large tile.
fn compress_offset_table(header: &Header, table_le: ByteVec) -> Result<ByteVec> {
    if !header.blocks.has_tiles() {
        return compress_unless_larger(header.compression, table_le);
    }

    let full_table_byte_size = full_block_table_byte_size(header).unwrap_or(usize::MAX);

    let is_partial_tile = table_le.len() < full_table_byte_size;
    if !is_partial_tile {
        return compress_unless_larger(header.compression, table_le);
    }

    if header.compression == Compression::Uncompressed {
        // the tile size is not limited, so do not pad tables of tiles much
        // larger than any practical tile, which would waste memory and space
        const MAX_PADDED_TABLE_BYTE_SIZE: usize = 16 * 1024 * 1024;

        // this limit also bounds the memory allocated for the padding
        let mut padded_table_le = table_le;
        if full_table_byte_size <= MAX_PADDED_TABLE_BYTE_SIZE {
            padded_table_le.resize(full_table_byte_size, 0);
        }

        return Ok(padded_table_le);
    }

    // any compressed size other than the size of the uncompressed table
    // of this block or of a full tile is recognized as compressed by all
    // readers
    let is_unambiguous = |compressed_le: &ByteVec| {
        compressed_le.len() != table_le.len() && compressed_le.len() < full_table_byte_size
    };

    let compressed_le = header.compression.compress_deep_bytes_le(&table_le)?;
    if is_unambiguous(&compressed_le) {
        return Ok(compressed_le);
    }

    let lengthened_le = header.compression.lengthen_compressed_deep_bytes_le(&compressed_le);
    Ok(lengthened_le.filter(is_unambiguous).unwrap_or(table_le))
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
        test_header_of_size(compression, blocks, Vec2(5, 3))
    }

    fn test_header_of_size(
        compression: Compression,
        blocks: BlockDescription,
        layer_size: Vec2<usize>,
    ) -> Header {
        let channels = smallvec![
            ChannelDescription::new("A", SampleType::F16, true),
            ChannelDescription::new("Z", SampleType::F32, false),
            ChannelDescription::new("id", SampleType::U32, false),
        ];

        let mut header = Header::new("deep".into(), layer_size, channels).with_encoding(
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
                let range = block.pixel_sample_byte_range(&header.channels, pixel, channel_index);
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

    /// Write a file with a flat and a deep layer, compressing the blocks of
    /// each layer with a separate call.
    #[cfg(feature = "rayon")]
    #[test]
    fn write_flat_and_deep_blocks_with_separate_parallel_compressors() {
        use crate::block::{writer::ChunksWriter, UncompressedBlock};

        let flat_header = Header::new(
            "flat".into(),
            Vec2(5, 3),
            smallvec![ChannelDescription::new("Y", SampleType::F32, false)],
        )
        .with_encoding(
            Compression::ZIP1,
            BlockDescription::ScanLines,
            LineOrder::Increasing,
        );

        let deep_header = test_header(Compression::ZIP1, BlockDescription::ScanLines);
        let headers = smallvec![flat_header, deep_header.clone()];

        let mut bytes = Vec::new();
        crate::block::write(std::io::Cursor::new(&mut bytes), headers, true, |meta, writer| {
            let deep_blocks = crate::block::enumerate_ordered_header_block_indices(&meta.headers)
                .filter(|(_, index)| index.layer == 1)
                .map(|(index_in_header, index)| {
                    let counts = vec![2; index.pixel_size.area()];
                    (index_in_header, test_block(&deep_header, index, &counts))
                });

            writer.on_progress(|_| ()).compress_all_blocks_parallel(&meta, deep_blocks)?;

            let flat_blocks = crate::block::enumerate_ordered_header_block_indices(&meta.headers)
                .filter(|(_, index)| index.layer == 0)
                .map(|(index_in_header, index)| {
                    let data = vec![0; index.pixel_size.area() * 4];
                    (
                        index_in_header,
                        UncompressedBlock {
                            index,
                            data,
                        },
                    )
                });

            writer.on_progress(|_| ()).compress_all_blocks_sequential(&meta, flat_blocks)
        })
        .unwrap();

        let image = crate::image::read::read()
            .deep_data()
            .first_valid_layer()
            .pedantic()
            .from_buffered(std::io::Cursor::new(bytes))
            .unwrap();

        assert_eq!(image.layer_data.channel_data.total_sample_count(), 5 * 3 * 2);
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
        assert_eq!(block.pixel_sample_byte_range(channels, 0, 0), 0..2);
        assert_eq!(block.pixel_sample_byte_range(channels, 1, 0), 2..6);
        assert_eq!(block.pixel_sample_byte_range(channels, 0, 1), 6..10);
        assert_eq!(block.pixel_sample_byte_range(channels, 1, 1), 10..18);
        assert_eq!(block.pixel_sample_byte_range(channels, 1, 2), 22..30);

        // second row: the first pixel has no samples
        assert_eq!(block.pixel_sample_byte_range(channels, 2, 0), 30..30);
        assert_eq!(block.pixel_sample_byte_range(channels, 3, 0), 30..32);
        assert_eq!(block.pixel_sample_byte_range(channels, 3, 1), 32..36);
        assert_eq!(block.pixel_sample_byte_range(channels, 3, 2), 36..40);
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
        let first = original.pixel_sample_byte_range(&header.channels, 0, 0);
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

        // a valid block whose size does not match the blocks of the header
        let block = test_block(
            &header,
            BlockIndex {
                pixel_size: Vec2(4, 1),
                ..index
            },
            &[1, 0, 3, 2],
        );
        block.validate(&header.channels).unwrap();
        assert!(matches!(
            block.compress_to_chunk(&meta.headers),
            Err(Error::Invalid(message)) if message.contains("does not match the header")
        ));

        // a valid block whose position does not match the blocks of the header
        let block = test_block(
            &header,
            BlockIndex {
                pixel_position: Vec2(1, 0),
                pixel_size: Vec2(4, 1),
                ..index
            },
            &[1, 0, 3, 2],
        );
        block.validate(&header.channels).unwrap();
        assert!(block.compress_to_chunk(&meta.headers).is_err());
    }

    fn tiles(size: usize) -> BlockDescription {
        BlockDescription::Tiles(TileDescription {
            tile_size: Vec2(size, size),
            level_mode: LevelMode::Singular,
            rounding_mode: crate::math::RoundingMode::Down,
        })
    }

    fn table_byte_size(chunk: &Chunk) -> usize {
        match &chunk.compressed_block {
            CompressedBlock::DeepTile(tile) => tile.compressed_pixel_offset_table.len(),
            _ => panic!("expected a deep tile"),
        }
    }

    fn set_table(chunk: &mut Chunk, table: &[u8]) {
        match &mut chunk.compressed_block {
            CompressedBlock::DeepTile(tile) => {
                tile.compressed_pixel_offset_table =
                    table.iter().map(|&byte| i8::from_ne_bytes([byte])).collect();
            }
            _ => panic!("expected a deep tile"),
        }
    }

    #[test]
    fn offset_tables_of_partial_tiles_are_readable_by_all_openexr_versions() {
        // the empty partial tiles are the cases where the compressed table is
        // exactly as large as the uncompressed table
        let sample_count_patterns: [&[u32]; 5] =
            [&[1], &[0], &[0, 0, 1, 0], &[2, 2, 2, 2, 2, 0], &[3, 0, 255, 1, 70000, 9, 0]];

        // the tiles at the right and bottom edge are partial
        let images = [
            (Vec2(5, 3), 2, vec![Compression::Uncompressed, Compression::RLE]),
            (Vec2(5, 4), 3, vec![Compression::Uncompressed, Compression::RLE, Compression::ZIP1]),
        ];

        for (layer_size, tile_size, compression_methods) in images {
            for compression in compression_methods {
                let header = test_header_of_size(compression, tiles(tile_size), layer_size);
                let meta = meta_data(header.clone());

                for counts in sample_count_patterns {
                    for (_, index) in
                        crate::block::enumerate_ordered_header_block_indices(&meta.headers)
                    {
                        let counts: Vec<u32> = (0..index.pixel_size.area())
                            .map(|pixel| counts[pixel % counts.len()])
                            .collect();

                        let block = test_block(&header, index, &counts);
                        let chunk = block.clone().compress_to_chunk(&meta.headers).unwrap();

                        let table_size = table_byte_size(&chunk);
                        let exact_size = index.pixel_size.area() * 4;
                        let full_tile_size = tile_size * tile_size * 4;

                        if compression == Compression::Uncompressed {
                            // like the `OpenEXR` library, pad uncompressed
                            // tables to the full tile size
                            assert_eq!(table_size, full_tile_size, "{index:?}");
                        } else if exact_size < full_tile_size {
                            // never store a partial table uncompressed,
                            // as older readers would misinterpret it as
                            // compressed
                            assert_ne!(table_size, exact_size, "{compression:?} {index:?}");
                            assert!(table_size < full_tile_size, "{:?} {:?}", compression, index);
                        }

                        let decompressed =
                            UncompressedDeepBlock::decompress_chunk(chunk, &meta, true).unwrap();
                        assert_eq!(decompressed, block);
                    }
                }
            }
        }
    }

    #[test]
    fn do_not_pad_offset_tables_of_huge_tiles() {
        for tile_size in [1 << 12, 1 << 15, 1 << 29] {
            let header =
                test_header_of_size(Compression::Uncompressed, tiles(tile_size), Vec2(1, 1));

            let meta = meta_data(header.clone());
            let index = BlockIndex {
                layer: 0,
                pixel_position: Vec2(0, 0),
                pixel_size: Vec2(1, 1),
                level: Vec2(0, 0),
            };

            let block = test_block(&header, index, &[2]);
            let chunk = block.clone().compress_to_chunk(&meta.headers).unwrap();
            assert_eq!(table_byte_size(&chunk), 4);

            let decompressed = UncompressedDeepBlock::decompress_chunk(chunk, &meta, true).unwrap();
            assert_eq!(decompressed, block);
        }
    }

    #[test]
    fn read_offset_tables_of_partial_tiles_written_by_older_openexr_versions() {
        for compression in [Compression::RLE, Compression::ZIP1] {
            let header = test_header(compression, tiles(2));
            let meta = meta_data(header.clone());

            for (_, index) in crate::block::enumerate_ordered_header_block_indices(&meta.headers) {
                for counts in [vec![0; index.pixel_size.area()], vec![2; index.pixel_size.area()]] {
                    let block = test_block(&header, index, &counts);
                    let chunk = block.clone().compress_to_chunk(&meta.headers).unwrap();
                    let table_le = offset_table_to_rows_le(
                        &block.pixel_offset_table,
                        index.pixel_size.width(),
                    )
                    .unwrap();

                    // older versions store the table without compression
                    // only if compression does not make it smaller than the
                    // table of a full tile, and then pad it to that size
                    let mut padded_table_le = table_le.clone();
                    padded_table_le.resize(2 * 2 * 4, 0);

                    let mut padded_chunk = chunk.clone();
                    set_table(&mut padded_chunk, &padded_table_le);

                    let decompressed =
                        UncompressedDeepBlock::decompress_chunk(padded_chunk, &meta, true).unwrap();
                    assert_eq!(decompressed, block, "{compression:?} {index:?}");

                    // older versions store a compressed table even if it is
                    // exactly as large as the uncompressed table
                    let compressed_table_le =
                        compression.compress_deep_bytes_le(&table_le).unwrap();
                    let mut compressed_chunk = chunk;
                    set_table(&mut compressed_chunk, &compressed_table_le);

                    let decompressed =
                        UncompressedDeepBlock::decompress_chunk(compressed_chunk, &meta, true)
                            .unwrap();
                    assert_eq!(decompressed, block, "{compression:?} {index:?}");
                }
            }
        }

        // the table of an empty 1x1 tile, compressed using RLE,
        // has exactly the size of the uncompressed table
        let table_le = [0; 4];
        assert_eq!(Compression::RLE.compress_deep_bytes_le(&table_le).unwrap().len(), 4);
    }

    #[test]
    fn lengthened_compressed_bytes_decompress_to_the_same_bytes() {
        let byte_patterns: [Vec<u8>; 6] = [
            vec![0; 4],
            vec![1, 2, 3, 4, 5, 6, 7, 8],
            vec![7; 300],
            (0..=255).collect(),
            (0..1000).map(|index: u32| (index * index % 7) as u8).collect(),
            vec![0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0],
        ];

        for compression in [Compression::RLE, Compression::ZIP1] {
            for bytes in &byte_patterns {
                let compressed = compression.compress_deep_bytes_le(bytes).unwrap();
                let lengthened =
                    compression.lengthen_compressed_deep_bytes_le(&compressed).unwrap();

                assert!(lengthened.len() > compressed.len(), "{:?} {:?}", compression, bytes);
                assert!(lengthened.len() <= compressed.len() + 5, "{:?} {:?}", compression, bytes);

                let decompressed =
                    compression.decompress_deep_bytes_le(&lengthened, bytes.len(), true).unwrap();
                assert_eq!(&decompressed, bytes, "{compression:?}");
            }
        }

        // every token of this stream produces a single byte
        let single_bytes = [-1_i8 as u8, 3, 0, 4];
        assert_eq!(Compression::RLE.lengthen_compressed_deep_bytes_le(&single_bytes), None);
    }

    #[test]
    fn reject_flat_blocks_in_deep_layers() {
        let header = test_header(Compression::ZIP1, BlockDescription::ScanLines);
        let meta = meta_data(header);
        let index = BlockIndex {
            layer: 0,
            pixel_position: Vec2(0, 0),
            pixel_size: Vec2(5, 1),
            level: Vec2(0, 0),
        };

        let block = crate::block::UncompressedBlock {
            index,
            data: vec![0; 5 * 10],
        };

        assert!(block.compress_to_chunk(&meta.headers).is_err());
    }

    #[test]
    fn reject_tile_indices_that_overflow() {
        let tile = TileCoordinates {
            tile_index: Vec2(usize::MAX / 2, 0),
            level_index: Vec2(0, 0),
        };

        assert!(tile.to_data_indices(Vec2(4, 4), Vec2(8, 8)).is_err());
    }
}
