
//! Describes all meta data possible in an exr file.

pub mod attributes;


use crate::io::*;
use ::smallvec::SmallVec;
use self::attributes::*;
use crate::chunks::{TileCoordinates, Block};
use crate::error::*;
use std::fs::File;
use std::io::{BufReader};
use crate::math::*;
use std::collections::{HashSet, HashMap};



/// Contains the complete meta data of an exr image.
/// Defines how the image is split up in the file,
/// the number and type of images and channels,
/// and various other attributes.
/// The usage of custom attributes is encouraged.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaData {

    /// Some flags summarizing the features that must be supported to decode the file.
    pub requirements: Requirements,

    /// One header to describe each layer in this file.
    pub headers: Headers,
}


/// List of `Header`s.
pub type Headers = SmallVec<[Header; 3]>;

/// List of `OffsetTable`s.
pub type OffsetTables = SmallVec<[OffsetTable; 3]>;


/// The offset table is an ordered list of indices referencing pixel data in the exr file.
/// For each pixel tile in the image, an index exists, which points to the byte-location
/// of the corresponding pixel data in the file. That index can be used to load specific
/// portions of an image without processing all bytes in a file. For each header,
/// an offset table exists with its indices ordered by `LineOrder::Increasing`.
// If the multipart bit is unset and the chunkCount attribute is not present,
// the number of entries in the chunk table is computed using the
// dataWindow, tileDesc, and compression attribute.
//
// If the multipart bit is set, the header must contain a
// chunkCount attribute, that contains the length of the offset table.
pub type OffsetTable = Vec<u64>;

/// Describes a single layer in a file.
/// A file can have any number of layers.
/// The meta data contains one header per layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {

    /// List of channels in this layer.
    pub channels: ChannelList,

    /// How the pixel data of all channels in this layer is compressed. May be `Compression::Uncompressed`.
    pub compression: Compression,

    /// Describes how the pixels of this layer are divided into smaller blocks.
    /// A single block can be loaded without processing all bytes of a file.
    ///
    /// Also describes whether a file contains multiple resolution levels: mip maps or rip maps.
    /// This allows loading not the full resolution, but the smallest sensible resolution.
    //
    // Required if file contains deep data or multiple layers.
    // Note: This value must agree with the version field's tile bit and deep data bit.
    // In this crate, this attribute will always have a value, for simplicity.
    pub blocks: Blocks,

    /// In what order the tiles of this header occur in the file.
    pub line_order: LineOrder,

    /// The resolution of this layer. Equals the size of the data window.
    pub data_size: Vec2<usize>,

    /// Whether this layer contains deep data.
    pub deep: bool,

    /// This library supports only deep data version 1.
    pub deep_data_version: Option<i32>,

    /// Number of chunks, that is, scan line blocks or tiles, that this image has been divided into.
    /// This number is calculated once at the beginning
    /// of the read process or when creating a header object.
    ///
    /// This value includes all chunks of all resolution levels.
    ///
    ///
    /// __Warning__
    /// _This value is relied upon. You should probably use `Header::with_encoding`,
    /// which automatically updates the chunk count._
    pub chunk_count: usize,

    // Required for deep data (deepscanline and deeptile) layers.
    // Note: Since the value of "maxSamplesPerPixel"
    // maybe be unknown at the time of opening the
    // file, the value “ -1 ” is written to the file to
    // indicate an unknown value. When the file is
    // closed, this will be overwritten with the correct value.
    // If file writing does not complete
    // correctly due to an error, the value -1 will
    // remain. In this case, the value must be derived
    // by decoding each chunk in the layer
    /// Maximum number of samples in a single pixel in a deep image.
    pub max_samples_per_pixel: Option<usize>,

    /// Includes mandatory fields like pixel aspect or display window
    /// which must be the same for all layers.
    pub shared_attributes: ImageAttributes,

    /// Does not include the attributes required for reading the file contents.
    /// Excludes standard fields that must be the same for all headers.
    pub own_attributes: LayerAttributes,
}


/// Includes mandatory fields like pixel aspect or display window
/// which must be the same for all layers.
#[derive(Clone, PartialEq, Debug)]
pub struct ImageAttributes {

    /// The rectangle anywhere in the global infinite 2D space
    /// that clips all contents of the file.
    pub display_window: IntRect,

    /// Aspect ratio of each pixel in this header.
    pub pixel_aspect: f32, // TODO integrate into `list`

    /// Optional attributes. Contains custom attributes.
    /// Does not contain the attributes already present in the `ImageAttributes`.
    /// Contains only attributes that are standardized to be the same for all headers: chromaticities and time codes.
    pub list: Vec<Attribute>,
}

/// Does not include the attributes required for reading the file contents.
/// Excludes standard fields that must be the same for all headers.
#[derive(Clone, PartialEq, Debug)]
pub struct LayerAttributes {

    /// The name of this layer.
    /// Required if this file contains deep data or multiple layers.
    // As this is an attribute value, it is not restricted in length, may even be empty
    pub name: Option<Text>,

    /// The bottom left corner of the rectangle that positions this layer
    /// within the global infinite 2D space of the whole file.
    /// Equals the position of the data window.
    pub data_position: Vec2<i32>,

    /// Part of the perspective projection. Default should be `(0, 0)`.
    // TODO same for all layers?
    pub screen_window_center: Vec2<f32>, // TODO integrate into `list`

    /// Part of the perspective projection. Default should be `1`.
    // TODO same for all layers?
    pub screen_window_width: f32, // TODO integrate into `list`

    /// Optional attributes. Contains custom attributes.
    /// Does not contain the attributes already present in the `Header` or `Attributes` struct.
    /// Does not contain attributes that are standardized to be the same for all layers: no chromaticities and no time codes.
    pub list: Vec<Attribute>,
}

/// A summary of requirements that must be met to read this exr file.
/// Used to determine whether this file can be read by a given reader.
/// It includes the OpenEXR version number. This library aims to support version `2.0`.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Requirements {

    /// This library supports reading version 1 and 2, and writing version 2.
    // TODO write version 1 for simple images
    file_format_version: u8,

    /// If true, this image has tiled blocks and contains only a single layer.
    /// If false and not deep and not multilayer, this image is a single layer image with scan line blocks.
    is_single_layer_and_tiled: bool,

    // in c or bad c++ this might have been relevant (omg is he allowed to say that)
    /// Whether this file has strings with a length greater than 31.
    /// Strings can never be longer than 255.
    has_long_names: bool,

    /// This image contains at least one layer with deep data.
    has_deep_data: bool,

    /// Whether this file contains multiple layers.
    has_multiple_layers: bool,
}


/// Locates a rectangular section of pixels in an image.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct TileIndices {

    /// Index of the tile.
    pub location: TileCoordinates,

    /// Pixel size of the tile.
    pub size: Vec2<usize>,
}

/// How the image pixels are split up into separate blocks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Blocks {

    /// The image is divided into scan line blocks.
    /// The number of scan lines in a block depends on the compression method.
    ScanLines,

    /// The image is divided into tile blocks.
    /// Also specifies the size of each tile in the image
    /// and whether this image contains multiple resolution levels.
    Tiles(TileDescription)
}


/*impl TileIndices {
    pub fn cmp(&self, other: &Self) -> Ordering {
        match self.location.level_index.1.cmp(&other.location.level_index.1) {
            Ordering::Equal => {
                match self.location.level_index.0.cmp(&other.location.level_index.0) {
                    Ordering::Equal => {
                        match self.location.tile_index.1.cmp(&other.location.tile_index.1) {
                            Ordering::Equal => {
                                self.location.tile_index.0.cmp(&other.location.tile_index.0)
                            },

                            other => other,
                        }
                    },

                    other => other
                }
            },

            other => other
        }
    }
}*/

impl Blocks {

    /// Whether this image is tiled. If false, this image is divided into scan line blocks.
    pub fn has_tiles(&self) -> bool {
        match self {
            Blocks::Tiles { .. } => true,
            _ => false
        }
    }
}

impl LayerAttributes {

    /// Create default layer attributes with a data position of zero.
    pub fn new(layer_name: Text) -> Self {
        Self {
            name: Some(layer_name),
            data_position: Vec2(0, 0),
            screen_window_center: Vec2(0.0, 0.0),
            screen_window_width: 1.0,
            list: Vec::new()
        }
    }

    /// Set the data position of this layer.
    pub fn with_position(self, data_position: Vec2<i32>) -> Self {
        Self { data_position, ..self }
    }
}

impl ImageAttributes {

    /// Create default image attributes with the specified display window size.
    /// The display window position is set to zero.
    pub fn new(display_size: Vec2<usize>) -> Self {
        Self {
            display_window: IntRect::from_dimensions(display_size),
            pixel_aspect: 1.0,
            list: Vec::new()
        }
    }

    /// Set the data position of this layer.
    pub fn with_display_window(self, display_window: IntRect) -> Self {
        Self { display_window, ..self }
    }
}


/// The first four bytes of each exr file.
/// Used to abort reading non-exr files.
pub mod magic_number {
    use super::*;

    /// The first four bytes of each exr file.
    pub const BYTES: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];

    /// Without validation, write this instance to the byte stream.
    pub fn write(write: &mut impl Write) -> Result<()> {
        u8::write_slice(write, &self::BYTES)
    }

    /// Consumes four bytes from the reader and returns whether the file may be an exr file.
    // TODO check if exr before allocating BufRead
    pub fn is_exr(read: &mut impl Read) -> Result<bool> {
        let mut magic_num = [0; 4];
        u8::read_slice(read, &mut magic_num)?;
        Ok(magic_num == self::BYTES)
    }

    /// Validate this image. If it is an exr file, return `Ok(())`.
    pub fn validate_exr(read: &mut impl Read) -> UnitResult {
        if self::is_exr(read)? {
            Ok(())

        } else {
            Err(Error::invalid("file identifier missing"))
        }
    }
}

/// A `0_u8` at the end of a sequence.
pub mod sequence_end {
    use super::*;

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        1
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(write: &mut W) -> UnitResult {
        0_u8.write(write)
    }

    /// Peeks the next byte. If it is zero, consumes the byte and returns true.
    pub fn has_come(read: &mut PeekRead<impl Read>) -> Result<bool> {
        Ok(read.skip_if_eq(0)?)
    }
}


fn missing_attribute(name: &str) -> Error {
    Error::invalid(format!("missing `{}` attribute", name))
}


/// Compute the number of tiles required to contain all values.
pub fn compute_block_count(full_res: usize, tile_size: usize) -> usize {
    // round up, because if the image is not evenly divisible by the tiles,
    // we add another tile at the end (which is only partially used)
    RoundingMode::Up.divide(full_res, tile_size)
}

/// Compute the start position and size of a block inside a dimension.
#[inline]
pub fn calculate_block_position_and_size(total_size: usize, block_size: usize, block_index: usize) -> Result<(usize, usize)> {
    let block_position = block_size * block_index;

    Ok((
        block_position,
        calculate_block_size(total_size, block_size, block_position)?
    ))
}

/// Calculate the size of a single block. If this is the last block,
/// this only returns the required size, which is always smaller than the default block size.
// TODO use this method everywhere instead of convoluted formulas
#[inline]
pub fn calculate_block_size(total_size: usize, block_size: usize, block_position: usize) -> Result<usize> {
    if block_position >= total_size {
        return Err(Error::invalid("block index"))
    }

    if block_position + block_size <= total_size {
        Ok(block_size)
    }
    else {
        Ok(total_size - block_position)
    }
}


/// Calculate number of mip levels in a given resolution.
// TODO this should be cached? log2 may be very expensive
pub fn compute_level_count(round: RoundingMode, full_res: usize) -> usize {
    round.log2(full_res) + 1
}

/// Calculate the size of a single mip level by index.
// TODO this should be cached? log2 may be very expensive
pub fn compute_level_size(round: RoundingMode, full_res: usize, level_index: usize) -> usize {
    round.divide(full_res,  1 << level_index).max(1)
}

/// Iterates over all rip map level resolutions of a given size, including the indices of each level.
/// The order of iteration conforms to `LineOrder::Increasing`.
// TODO cache these?
// TODO compute these directly instead of summing up an iterator?
pub fn rip_map_levels(round: RoundingMode, max_resolution: Vec2<usize>) -> impl Iterator<Item=(Vec2<usize>, Vec2<usize>)> {
    rip_map_indices(round, max_resolution).map(move |level_indices|{
        // TODO progressively divide instead??
        let width = compute_level_size(round, max_resolution.0, level_indices.0);
        let height = compute_level_size(round, max_resolution.1, level_indices.1);
        (level_indices, Vec2(width, height))
    })
}

/// Iterates over all mip map level resolutions of a given size, including the indices of each level.
/// The order of iteration conforms to `LineOrder::Increasing`.
// TODO cache all these level values when computing table offset size??
// TODO compute these directly instead of summing up an iterator?
pub fn mip_map_levels(round: RoundingMode, max_resolution: Vec2<usize>) -> impl Iterator<Item=(usize, Vec2<usize>)> {
    mip_map_indices(round, max_resolution)
        .map(move |level_index|{
            // TODO progressively divide instead??
            let width = compute_level_size(round, max_resolution.0, level_index);
            let height = compute_level_size(round, max_resolution.1, level_index);
            (level_index, Vec2(width, height))
        })
}

/// Iterates over all rip map level indices of a given size.
/// The order of iteration conforms to `LineOrder::Increasing`.
pub fn rip_map_indices(round: RoundingMode, max_resolution: Vec2<usize>) -> impl Iterator<Item=Vec2<usize>> {
    let (width, height) = (
        compute_level_count(round, max_resolution.0),
        compute_level_count(round, max_resolution.1)
    );

    (0..height).flat_map(move |y_level|{
        (0..width).map(move |x_level|{
            Vec2(x_level, y_level)
        })
    })
}

/// Iterates over all mip map level indices of a given size.
/// The order of iteration conforms to `LineOrder::Increasing`.
pub fn mip_map_indices(round: RoundingMode, max_resolution: Vec2<usize>) -> impl Iterator<Item=usize> {
    (0..compute_level_count(round, max_resolution.0.max(max_resolution.1)))
}

/// Compute the number of chunks that an image is divided into. May be an expensive operation.
// If not multilayer and chunkCount not present,
// the number of entries in the chunk table is computed
// using the dataWindow and tileDesc attributes and the compression format
pub fn compute_chunk_count(compression: Compression, data_size: Vec2<usize>, blocks: Blocks) -> usize {

    if let Blocks::Tiles(tiles) = blocks {
        let round = tiles.rounding_mode;
        let Vec2(tile_width, tile_height) = tiles.tile_size;

        // TODO cache all these level values??
        use crate::meta::attributes::LevelMode::*;
        match tiles.level_mode {
            Singular => {
                let tiles_x = compute_block_count(data_size.0, tile_width);
                let tiles_y = compute_block_count(data_size.1, tile_height);
                tiles_x * tiles_y
            }

            MipMap => {
                mip_map_levels(round, data_size).map(|(_, Vec2(level_width, level_height))| {
                    compute_block_count(level_width, tile_width) * compute_block_count(level_height, tile_height)
                }).sum()
            },

            RipMap => {
                rip_map_levels(round, data_size).map(|(_, Vec2(level_width, level_height))| {
                    compute_block_count(level_width, tile_width) * compute_block_count(level_height, tile_height)
                }).sum()
            }
        }
    }

    // scan line blocks never have mip maps
    else {
        compute_block_count(data_size.1, compression.scan_lines_per_block())
    }
}



impl MetaData {

    /// Infers version requirements from headers.
    pub fn new(headers: Headers) -> Self {
        MetaData {
            requirements: Requirements::infer(headers.as_slice()),
            headers
        }
    }

    /// Read the exr meta data from a file.
    /// Use `read_from_unbuffered` instead if you do not have a file.
    /// Does not validate the meta data.
    #[must_use]
    pub fn read_from_file(path: impl AsRef<::std::path::Path>) -> Result<Self> {
        Self::read_from_unbuffered(File::open(path)?)
    }

    /// Buffer the reader and then read the exr meta data from it.
    /// Use `read_from_buffered` if your reader is an in-memory reader.
    /// Use `read_from_file` if you have a file path.
    /// Does not validate the meta data.
    #[must_use]
    pub fn read_from_unbuffered(unbuffered: impl Read) -> Result<Self> {
        Self::read_from_buffered(BufReader::new(unbuffered))
    }

    /// Read the exr meta data from a reader.
    /// Use `read_from_file` if you have a file path.
    /// Use `read_from_unbuffered` if this is not an in-memory reader.
    /// Does not validate the meta data.
    #[must_use]
    pub fn read_from_buffered(buffered: impl Read) -> Result<Self> {
        let mut read = PeekRead::new(buffered);
        MetaData::read_unvalidated_from_buffered_peekable(&mut read)
    }

    /// Does __not validate__ the meta data.
    #[must_use]
    pub(crate) fn read_unvalidated_from_buffered_peekable(read: &mut PeekRead<impl Read>) -> Result<Self> {
        magic_number::validate_exr(read)?;
        let requirements = Requirements::read(read)?;
        let headers = Header::read_all(read, &requirements)?;

        // TODO check if supporting requirements 2 always implies supporting requirements 1

        // TODO only validate the read data that may produce errors later on,
        //      not because of missing attributes that nobody needs

        Ok(MetaData { requirements, headers })
    }

    /// Validates the meta data.
    #[must_use]
    pub(crate) fn read_from_buffered_peekable(read: &mut PeekRead<impl Read>) -> Result<Self> {
        let meta_data = Self::read_unvalidated_from_buffered_peekable(read)?;

        // relaxed validation to allow slightly invalid files
        // that still can be read correctly
        meta_data.validate(false)?;

        Ok(meta_data)
    }

    /// Validates the meta data and writes it to the stream.
    /// If pedantic, throws errors for files that may produce errors in other exr readers.
    pub(crate) fn write_validating_to_buffered(&self, write: &mut impl Write, pedantic: bool) -> UnitResult {
        // pedantic validation to not allow slightly invalid files
        // that still could be read correctly in theory
        self.validate(pedantic)?;

        magic_number::write(write)?;
        self.requirements.write(write)?;
        Header::write_all(self.headers.as_slice(), write, self.requirements.has_multiple_layers)?;
        Ok(())
    }

    /// Read one offset table from the reader for each header.
    pub fn read_offset_tables(read: &mut PeekRead<impl Read>, headers: &Headers) -> Result<OffsetTables> {
        headers.iter()
            .map(|header| u64::read_vec(read, header.chunk_count, std::u16::MAX as usize, None))
            .collect()
    }

    /// Skip the offset tables by advancing the reader by the required byte count.
    // TODO use seek for large (probably all) tables!
    pub fn skip_offset_tables(read: &mut PeekRead<impl Read>, headers: &Headers) -> Result<usize> {
        let chunk_count: usize = headers.iter().map(|header| header.chunk_count).sum();
        crate::io::skip_bytes(read, chunk_count * u64::BYTE_SIZE)?;
        Ok(chunk_count)
    }

    /// Validates this meta data.
    /// Set strict to false when reading and true when writing for maximum compatibility.
    pub fn validate(&self, strict: bool) -> UnitResult {
        self.requirements.validate()?;

        let headers = self.headers.len();

        if headers == 0 {
            return Err(Error::invalid("at least one layer is required"));
        }

        for header in &self.headers {
            header.validate(&self.requirements, strict)?;
        }

        if strict { // check for duplicate header names
            let mut header_names = HashSet::with_capacity(headers);
            for header in &self.headers {
                if !header_names.insert(&header.own_attributes.name) {
                    return Err(Error::invalid(format!(
                        "duplicate layer name: `{}`",
                        header.own_attributes.name.as_ref().expect("header validation bug")
                    )));
                }
            }
        }

        if strict {
            let must_share = self.headers.iter().flat_map(|header| header.own_attributes.list.iter())
                .any(|attribute| attribute.value.to_chromaticities().is_ok() || attribute.value.to_time_code().is_ok());

            if must_share {
                return Err(Error::invalid("chromaticities and time code attributes must must not exist in own attributes but shared instead"));
            }
        }

        if strict && headers > 1 { // check for attributes that should not differ in between headers
            fn get_attributes(header: &'_ Header) -> HashMap<&'_ [u8], &'_ AnyValue> {
                header.shared_attributes.list.iter()
                    .map(|attribute| (attribute.name.bytes(), &attribute.value))
                    .collect()
            };

            let first_header = self.headers.first().expect("header count validation bug");
            let first_header_attributes = get_attributes(first_header);

            for header in &self.headers[1..] {
                let attributes = get_attributes(header);
                if attributes != first_header_attributes
                    || header.shared_attributes.display_window != first_header.shared_attributes.display_window
                    || header.shared_attributes.pixel_aspect != first_header.shared_attributes.pixel_aspect
                {
                    return Err(Error::invalid("display window, pixel aspect, chromaticities, and time code attributes must be equal for all headers"))
                }
            }
        }

        if self.requirements.file_format_version == 1 || !self.requirements.has_multiple_layers {
            if headers != 1 {
                return Err(Error::invalid("multipart flag for header count"));
            }
        }

        Ok(())
    }
}



impl Header {

    /// Create a new Header with the specified name, display window and channels.
    /// Use `Header::with_encoding` and the similar methods to add further properties to the header.
    ///
    /// The other settings are left to their default values:
    /// - no compression
    /// - display window equal to data window
    /// - scan line blocks
    /// - unspecified line order
    /// - no custom attributes
    pub fn new(name: Text, data_size: Vec2<usize>, channels: SmallVec<[Channel; 5]>) -> Self {
        let compression = Compression::Uncompressed;
        let blocks = Blocks::ScanLines;

        Self {
            data_size,
            compression,
            blocks,

            channels: ChannelList::new(channels),
            line_order: LineOrder::Unspecified,

            shared_attributes: ImageAttributes { // TODO use  LayerAttributes::new(data_size)
                display_window: IntRect::new(Vec2(0, 0), data_size),
                pixel_aspect: 1.0,
                list: Vec::new(),
            },

            own_attributes: LayerAttributes { // TODO use  LayerAttributes::new(name)
                name: Some(name),
                data_position: Vec2(0,0),
                screen_window_center: Vec2(0.0, 0.0),
                screen_window_width: 1.0,
                list: Vec::new()
            },

            chunk_count: compute_chunk_count(compression, data_size, blocks),

            deep: false,
            deep_data_version: None,
            max_samples_per_pixel: None,
        }
    }

    /// Set the display window, that is, the global clipping rectangle.
    /// __Must be the same for all headers of a file.__
    pub fn with_display_window(mut self, display_window: IntRect) -> Self {
        self.shared_attributes.display_window = display_window;
        self
    }

    /// Set the offset of this layer.
    pub fn with_position(mut self, position: Vec2<i32>) -> Self {
        self.own_attributes.data_position = position;
        self
    }

    /// Set compression, tiling, and line order. Automatically computes chunk count.
    pub fn with_encoding(self, compression: Compression, blocks: Blocks, line_order: LineOrder) -> Self {
        Self {
            chunk_count: compute_chunk_count(compression, self.data_size, blocks),
            compression, blocks, line_order,
            .. self
        }
    }

    /// Add some custom attributes to the header that are not shared with all other headers in the image.
    pub fn with_attributes(self, own_attributes: LayerAttributes) -> Self {
        Self { own_attributes, .. self }
    }

    /// Add some custom attributes to the header that are shared with all other headers in the image.
    pub fn with_shared_attributes(self, shared_attributes: ImageAttributes) -> Self {
        Self { shared_attributes, .. self }
    }

    /// Iterate over all blocks, in the order specified by the headers line order attribute,
    /// with an index returning the original index of the block if it were `LineOrder::Increasing`.
    pub fn enumerate_ordered_blocks(&self) -> impl Iterator<Item = (usize, TileIndices)> + Send {
        let increasing_y = self.blocks_increasing_y_order().enumerate();

        let ordered: Box<dyn Send + Iterator<Item = (usize, TileIndices)>> = {
            if self.line_order == LineOrder::Decreasing {
                Box::new(increasing_y.rev()) // TODO without box?
            }
            else {
                Box::new(increasing_y)
            }
        };

        ordered
    }

    /// Iterate over all tile indices in this header in `LineOrder::Increasing` order.
    pub fn blocks_increasing_y_order(&self) -> impl Iterator<Item = TileIndices> + ExactSizeIterator + DoubleEndedIterator {
        fn tiles_of(image_size: Vec2<usize>, tile_size: Vec2<usize>, level_index: Vec2<usize>) -> impl Iterator<Item=TileIndices> {
            fn divide_and_rest(total_size: usize, block_size: usize) -> impl Iterator<Item=(usize, usize)> {
                let block_count = compute_block_count(total_size, block_size);
                (0..block_count).map(move |block_index| (
                    block_index, calculate_block_size(total_size, block_size, block_index).expect("block size calculation bug")
                ))
            }

            divide_and_rest(image_size.1, tile_size.1).flat_map(move |(y_index, tile_height)|{
                divide_and_rest(image_size.0, tile_size.0).map(move |(x_index, tile_width)|{
                    TileIndices {
                        size: Vec2(tile_width, tile_height),
                        location: TileCoordinates { tile_index: Vec2(x_index, y_index), level_index, },
                    }
                })
            })
        }

        let vec: Vec<TileIndices> = {
            if let Blocks::Tiles(tiles) = self.blocks {
                match tiles.level_mode {
                    LevelMode::Singular => {
                        tiles_of(self.data_size, tiles.tile_size, Vec2(0, 0)).collect()
                    },
                    LevelMode::MipMap => {
                        mip_map_levels(tiles.rounding_mode, self.data_size)
                            .flat_map(move |(level_index, level_size)|{
                                tiles_of(level_size, tiles.tile_size, Vec2(level_index, level_index))
                            })
                            .collect()
                    },
                    LevelMode::RipMap => {
                        rip_map_levels(tiles.rounding_mode, self.data_size)
                            .flat_map(move |(level_index, level_size)| {
                                tiles_of(level_size, tiles.tile_size, level_index)
                            })
                            .collect()
                    }
                }
            }
            else {
                let tiles = Vec2(self.data_size.0, self.compression.scan_lines_per_block());
                tiles_of(self.data_size, tiles, Vec2(0,0)).collect()
            }
        };

        vec.into_iter() // TODO without collect
    }

    /// Calculate the position of a block in the global infinite 2D space of a file. May be negative.
    pub fn get_block_data_window_coordinates(&self, tile: TileCoordinates) -> Result<IntRect> {
        let data = self.get_absolute_block_indices(tile)?;
        Ok(data.with_origin(self.own_attributes.data_position))
    }

    /// Calculate the pixel index rectangle inside this header. Is not negative. Starts at `0`.
    pub fn get_absolute_block_indices(&self, tile: TileCoordinates) -> Result<IntRect> {
        Ok(if let Blocks::Tiles(tiles) = self.blocks {
            let Vec2(data_width, data_height) = self.data_size;

            let data_width = compute_level_size(tiles.rounding_mode, data_width, tile.level_index.0);
            let data_height = compute_level_size(tiles.rounding_mode, data_height, tile.level_index.1);
            let absolute_tile_coordinates = tile.to_data_indices(tiles.tile_size, Vec2(data_width, data_height))?;

            if absolute_tile_coordinates.position.0 as i64 >= data_width as i64 || absolute_tile_coordinates.position.1 as i64 >= data_height as i64 {
                return Err(Error::invalid("data block tile index"))
            }

            absolute_tile_coordinates
        }
        else { // this is a scanline image
            debug_assert_eq!(tile.tile_index.0, 0, "block index calculation bug");

            let (y, height) = calculate_block_position_and_size(
                self.data_size.1,
                self.compression.scan_lines_per_block(),
                tile.tile_index.1
            )?;

            IntRect {
                position: Vec2(0, usize_to_i32(y)),
                size: Vec2(self.data_size.0, height)
            }
        })

        // TODO deep data?
    }

    /// Return the tile index, converting scan line block coordinates to tile indices.
    /// Starts at `0` and is not negative.
    pub fn get_block_data_indices(&self, block: &Block) -> Result<TileCoordinates> {
        Ok(match block {
            Block::Tile(ref tile) => {
                tile.coordinates
            },

            Block::ScanLine(ref block) => {
                let size = self.compression.scan_lines_per_block() as i32;
                let y = (block.y_coordinate - self.own_attributes.data_position.1) / size;

                if y < 0 {
                    panic!("y index calculation bug");
                }

                TileCoordinates {
                    tile_index: Vec2(0, y as usize),
                    level_index: Vec2(0, 0)
                }
            },

            _ => return Err(Error::unsupported("deep data not supported yet"))
        })
    }

    /// Maximum byte length of an uncompressed or compressed block, used for validation.
    pub fn max_block_byte_size(&self) -> usize {
        self.channels.bytes_per_pixel * match self.blocks {
            Blocks::Tiles(tiles) => tiles.tile_size.0 * tiles.tile_size.1,
            Blocks::ScanLines => self.compression.scan_lines_per_block() * self.data_size.0
            // TODO What about deep data???
        }
    }

    /// Validate this instance.
    pub fn validate(&self, requirements: &Requirements, strict: bool) -> UnitResult {
        debug_assert_eq!(
            self.chunk_count, compute_chunk_count(self.compression, self.data_size, self.blocks),
            "chunk count attribute not correctly set"
        );

        if strict && requirements.is_multilayer() {
            if self.own_attributes.name.is_none() {
                return Err(missing_attribute("layer name for multi layer file"));
            }
        }

        // TODO is this really a required?
        if strict && self.blocks == Blocks::ScanLines && self.line_order == LineOrder::Unspecified {
            return Err(Error::invalid("scan line images cannot have an unspecified line order"));
        }

        let allow_subsampling = !self.deep && self.blocks == Blocks::ScanLines;
        self.channels.validate(allow_subsampling, strict)?;

        for attribute in &self.shared_attributes.list {
            attribute.validate(requirements.has_long_names, allow_subsampling, strict)?;
        }

        for attribute in &self.own_attributes.list {
            attribute.validate(requirements.has_long_names, allow_subsampling, strict)?;
        }


        // check if attribute names appear twice
        if strict {
            let mut custom_names = HashSet::with_capacity(
                self.own_attributes.list.len() + self.shared_attributes.list.len()
            );

            for attribute in &self.own_attributes.list {
                if !custom_names.insert(attribute.name.bytes()) {
                    return Err(Error::invalid(format!("duplicate attribute name: `{}`", attribute.name)));
                }
            }

            for attribute in &self.shared_attributes.list {
                if !custom_names.insert(attribute.name.bytes()) {
                    return Err(Error::invalid(format!("duplicate attribute name: `{}`", attribute.name)));
                }
            }

            use attributes::required_attribute_names::*;
            let reserved_names = [
                TILES, NAME, BLOCK_TYPE, DEEP_DATA_VERSION, CHUNKS, MAX_SAMPLES, CHANNELS, COMPRESSION,
                DATA_WINDOW, DISPLAY_WINDOW, LINE_ORDER, PIXEL_ASPECT, WINDOW_CENTER, WINDOW_WIDTH
            ];


            for &reserved in &reserved_names {
                if custom_names.contains(reserved) {

                    return Err(Error::invalid(format!(
                        "attribute name `{}` is already a required attribute",
                         Text::from_bytes_unchecked(reserved.into())
                    )));
                }
            }
        }

        if self.deep {
            if strict && self.own_attributes.name.is_none() {
                return Err(missing_attribute("layer name for deep file"));
            }

            match self.deep_data_version {
                Some(1) => {},
                Some(_) => return Err(Error::unsupported("deep data version")),
                None => return Err(missing_attribute("deep data version")),
            }

            if strict && self.max_samples_per_pixel.is_none() {
                return Err(Error::invalid("missing max samples per pixel attribute for deepdata"));
            }

            if !self.compression.supports_deep_data() {
                return Err(Error::invalid("compression method does not support deep data"));
            }
        }

        Ok(())
    }

    /// Read the headers without validating them.
    pub fn read_all(read: &mut PeekRead<impl Read>, version: &Requirements) -> Result<Headers> {
        if !version.is_multilayer() {
            Ok(smallvec![ Header::read(read, version)? ])
        }
        else {
            let mut headers = SmallVec::new();

            while !sequence_end::has_come(read)? {
                headers.push(Header::read(read, version)?);
            }

            Ok(headers)
        }
    }

    /// Without validation, write the headers to the byte stream.
    pub fn write_all(headers: &[Header], write: &mut impl Write, is_multilayer: bool) -> UnitResult {
        for header in headers {
            header.write(write)?;
        }

        if is_multilayer {
            sequence_end::write(write)?;
        }

        Ok(())
    }

    /// Read the value without validating.
    pub fn read(read: &mut PeekRead<impl Read>, requirements: &Requirements) -> Result<Self> {
        let max_string_len = if requirements.has_long_names { 256 } else { 32 }; // TODO DRY this information

        let mut shared_custom = Vec::new();
        let mut own_custom = Vec::new();

        // these required attributes will be filled when encountered while parsing
        let mut tiles = None;
        let mut name = None;
        let mut block_type = None;
        let mut version = None;
        let mut chunk_count = None;
        let mut max_samples_per_pixel = None;
        let mut channels = None;
        let mut compression = None;
        let mut data_window = None;
        let mut display_window = None;
        let mut line_order = None;
        let mut pixel_aspect = None;
        let mut screen_window_center = None;
        let mut screen_window_width = None;

        // read each attribute in this header
        while !sequence_end::has_come(read)? {
            let Attribute { name: attribute_name, value } = Attribute::read(read, max_string_len)?;

            // if the attribute is a required attribute, set the corresponding variable directly.
            // otherwise, add the attribute to the vector of custom attributes
            use crate::meta::attributes::required_attribute_names::*;
            match attribute_name.bytes() {
                TILES => tiles = Some(value.to_tile_description()?),
                NAME => name = Some(value.into_text()?),
                BLOCK_TYPE => block_type = Some(BlockType::parse(value.into_text()?)?),
                CHANNELS => channels = Some(value.into_channel_list()?),
                COMPRESSION => compression = Some(value.to_compression()?),
                DATA_WINDOW => data_window = Some(value.to_i32_box_2()?),
                DISPLAY_WINDOW => display_window = Some(value.to_i32_box_2()?),
                LINE_ORDER => line_order = Some(value.to_line_order()?),
                PIXEL_ASPECT => pixel_aspect = Some(value.to_f32()?),
                WINDOW_CENTER => screen_window_center = Some(value.to_f32_vec_2()?),
                WINDOW_WIDTH => screen_window_width = Some(value.to_f32()?),
                DEEP_DATA_VERSION => version = Some(value.to_i32()?),

                MAX_SAMPLES => max_samples_per_pixel = Some(
                    i32_to_usize(value.to_i32()?, "max sample count")?
                ),

                CHUNKS => chunk_count = Some(
                    i32_to_usize(value.to_i32()?, "chunk count")?
                ),

                _ => {
                    if value.to_chromaticities().is_ok() || value.to_time_code().is_ok() {
                        shared_custom.push(Attribute { name: attribute_name, value })
                    }
                    else {
                        own_custom.push(Attribute { name: attribute_name, value })
                    }
                },
            }
        }

        let compression = compression.ok_or(missing_attribute("compression"))?;
        let data_window = data_window.ok_or(missing_attribute("data window"))?;
        let data_size = data_window.size;

        let blocks = match block_type {
            None if requirements.is_single_layer_and_tiled => {
                Blocks::Tiles(tiles.ok_or(missing_attribute("tiles"))?)
            },
            Some(BlockType::Tile) | Some(BlockType::DeepTile) => {
                Blocks::Tiles(tiles.ok_or(missing_attribute("tiles"))?)
            },

            _ => Blocks::ScanLines,
        };

        let chunk_count = match chunk_count {
            None => compute_chunk_count(compression, data_size, blocks),
            Some(count) => count,
        };


        let header = Header {
            compression,
            chunk_count,
            data_size,

            shared_attributes: ImageAttributes {
                display_window: display_window.ok_or(missing_attribute("display window"))?,
                pixel_aspect: pixel_aspect.unwrap_or(1.0),
                list: shared_custom
            },

            own_attributes: LayerAttributes {
                name,

                data_position: data_window.position,
                screen_window_center: screen_window_center.unwrap_or(Vec2(0.0, 0.0)),
                screen_window_width: screen_window_width.unwrap_or(1.0),
                list: own_custom,
            },

            channels: channels.ok_or(missing_attribute("channels"))?,
            line_order: line_order.unwrap_or(LineOrder::Unspecified),

            blocks,
            max_samples_per_pixel,
            deep_data_version: version,
            deep: block_type == Some(BlockType::DeepScanLine) || block_type == Some(BlockType::DeepTile),

        };

        Ok(header)
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write(&self, write: &mut impl Write) -> UnitResult {

        // FIXME do not allocate text object for writing!
        /// Write a mandatory attribute.
        fn write_attr<T>(write: &mut impl Write, name: &'static [u8], value: T, variant: impl Fn(T) -> AnyValue) -> UnitResult {
            Attribute::predefined(name, variant(value)).write(write)
        };

        /// Write an optional attribute without validation.
        fn write_opt_attr<T>(write: &mut impl Write, name: &'static [u8], attribute: Option<T>, variant: impl Fn(T) -> AnyValue) -> UnitResult {
            if let Some(value) = attribute { write_attr(write, name, value, variant) }
            else { Ok(()) }
        };

        {
            use crate::meta::attributes::required_attribute_names::*;
            use AnyValue::*;

            let (block_type, tiles) = match self.blocks {
                Blocks::ScanLines => (attributes::BlockType::ScanLine, None),
                Blocks::Tiles(tiles) => (attributes::BlockType::Tile, Some(tiles))
            };

            write_opt_attr(write, TILES, tiles, TileDescription)?;

            write_opt_attr(write, NAME, self.own_attributes.name.clone(), Text)?; // TODO no clone
            write_opt_attr(write, DEEP_DATA_VERSION, self.deep_data_version, I32)?;
            write_opt_attr(write, MAX_SAMPLES, self.max_samples_per_pixel, |u| I32(u as i32))?;

            // not actually required, but always computed in this library anyways
            write_attr(write, CHUNKS, self.chunk_count, |u| I32(u as i32))?;
            write_attr(write, BLOCK_TYPE, block_type, BlockType)?;

            write_attr(write, CHANNELS, self.channels.clone(), ChannelList)?; // FIXME do not clone
            write_attr(write, COMPRESSION, self.compression, Compression)?;
            write_attr(write, LINE_ORDER, self.line_order, LineOrder)?;

            write_attr(write, DATA_WINDOW, self.data_window(), IntRect)?;
            write_attr(write, DISPLAY_WINDOW, self.shared_attributes.display_window, IntRect)?;
            write_attr(write, PIXEL_ASPECT, self.shared_attributes.pixel_aspect, F32)?;
            write_attr(write, WINDOW_WIDTH, self.own_attributes.screen_window_width, F32)?;
            write_attr(write, WINDOW_CENTER, self.own_attributes.screen_window_center, FloatVec2)?;
        }

        for attrib in &self.shared_attributes.list {
            attrib.write(write)?;
        }

        for attrib in &self.own_attributes.list {
            attrib.write(write)?;
        }

        sequence_end::write(write)?;
        Ok(())
    }

    /// The rectangle describing the bounding box of this layer
    /// within the infinite global 2D space of the file.
    pub fn data_window(&self) -> IntRect {
        IntRect::new(self.own_attributes.data_position, self.data_size)
    }
}


impl Requirements {

    /// Infer version requirements from headers.
    pub fn infer(headers: &[Header]) -> Self {
        let first_header_has_tiles = headers.iter().next()
            .map_or(false, |header| header.blocks.has_tiles());

        let is_multilayer = headers.len() > 1;
        let deep = false; // TODO deep data

        Requirements {
            file_format_version: 2, // TODO find minimum
            is_single_layer_and_tiled: !is_multilayer && first_header_has_tiles,
            has_long_names: true, // TODO query header?
            has_multiple_layers: is_multilayer,
            has_deep_data: deep,
        }
    }


    // this is actually used for control flow, as the number of headers may be 1 in a multilayer file
    /// Is this file declared to contain multiple layers?
    pub fn is_multilayer(&self) -> bool {
        self.has_multiple_layers
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        use ::bit_field::BitField;

        let version_and_flags = u32::read(read)?;

        // take the 8 least significant bits, they contain the file format version number
        let version = (version_and_flags & 0x000F) as u8;

        // the 24 most significant bits are treated as a set of boolean flags
        let is_single_tile = version_and_flags.get_bit(9);
        let has_long_names = version_and_flags.get_bit(10);
        let has_deep_data = version_and_flags.get_bit(11);
        let has_multiple_layers = version_and_flags.get_bit(12);

        // all remaining bits except 9, 10, 11 and 12 are reserved and should be 0
        // if a file has any of these bits set to 1, it means this file contains
        // a feature that we don't support
        let unknown_flags = version_and_flags >> 13; // all flags excluding the 12 bits we already parsed

        if unknown_flags != 0 { // TODO test if this correctly detects unsupported files
            return Err(Error::unsupported("too new file feature flags"));
        }

        let version = Requirements {
            file_format_version: version,
            is_single_layer_and_tiled: is_single_tile, has_long_names,
            has_deep_data, has_multiple_layers,
        };

        Ok(version)
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(self, write: &mut W) -> UnitResult {
        use ::bit_field::BitField;

        // the 8 least significant bits contain the file format version number
        // and the flags are set to 0
        let mut version_and_flags = self.file_format_version as u32;

        // the 24 most significant bits are treated as a set of boolean flags
        version_and_flags.set_bit(9, self.is_single_layer_and_tiled);
        version_and_flags.set_bit(10, self.has_long_names);
        version_and_flags.set_bit(11, self.has_deep_data);
        version_and_flags.set_bit(12, self.has_multiple_layers);
        // all remaining bits except 9, 10, 11 and 12 are reserved and should be 0

        version_and_flags.write(write)?;
        Ok(())
    }

    /// Validate this instance.
    pub fn validate(&self) -> UnitResult {
        if self.has_deep_data { // TODO deep data (and then remove this check)
            return Err(Error::unsupported("deep data not supported yet"));
        }

        if let 1..=2 = self.file_format_version {

            match (
                self.is_single_layer_and_tiled, self.has_deep_data, self.has_multiple_layers,
                self.file_format_version
            ) {
                // Single-part scan line. One normal scan line image.
                (false, false, false, 1..=2) => Ok(()),

                // Single-part tile. One normal tiled image.
                (true, false, false, 1..=2) => Ok(()),

                // Multi-part (new in 2.0).
                // Multiple normal images (scan line and/or tiled).
                (false, false, true, 2) => Ok(()),

                // Single-part deep data (new in 2.0).
                // One deep tile or deep scan line part
                (false, true, false, 2) => Ok(()),

                // Multi-part deep data (new in 2.0).
                // Multiple parts (any combination of:
                // tiles, scan lines, deep tiles and/or deep scan lines).
                (false, true, true, 2) => Ok(()),

                _ => Err(Error::invalid("file feature flags"))
            }
        }
        else {
            Err(Error::unsupported("file version newer than `2.0`"))
        }

    }
}


#[cfg(test)]
mod test {
    use crate::meta::{MetaData, Requirements, Header, ImageAttributes, LayerAttributes, compute_chunk_count};
    use crate::meta::attributes::{Text, ChannelList, IntRect, LineOrder, Channel, PixelType};
    use crate::compression::Compression;
    use crate::meta::Blocks;
    use crate::math::*;

    #[test]
    fn round_trip_requirements() {
        let requirements = Requirements {
            file_format_version: 2,
            is_single_layer_and_tiled: true,
            has_long_names: false,
            has_deep_data: true,
            has_multiple_layers: false
        };

        let mut data: Vec<u8> = Vec::new();
        requirements.write(&mut data).unwrap();
        let read = Requirements::read(&mut data.as_slice()).unwrap();
        assert_eq!(requirements, read);
    }

    #[test]
    fn round_trip(){
        let header = Header {
            channels: ChannelList {
                list: smallvec![
                    Channel {
                        name: Text::from("main").unwrap(),
                        pixel_type: PixelType::U32,
                        is_linear: false,
                        sampling: Vec2(1, 1)
                    }
                ],
                bytes_per_pixel: 4
            },
            compression: Compression::Uncompressed,
            line_order: LineOrder::Increasing,
            deep_data_version: Some(1),
            chunk_count: compute_chunk_count(Compression::Uncompressed, Vec2(2000, 333), Blocks::ScanLines),
            max_samples_per_pixel: Some(4),
            shared_attributes: ImageAttributes {
                display_window: IntRect {
                    position: Vec2(2,1),
                    size: Vec2(11, 9)
                },
                pixel_aspect: -3.0,
                list: vec![ /* TODO */ ]
            },

            blocks: Blocks::ScanLines,
            deep: false,
            data_size: Vec2(2000, 333),
            own_attributes: LayerAttributes {
                name: Some(Text::from("test name lol").unwrap()),
                data_position: Vec2(3, -5),
                screen_window_center: Vec2(0.3, 99.0),
                screen_window_width: -0.19,
                list: vec![ /* TODO */ ]
            }
        };

        let meta = MetaData {
            requirements: Requirements {
                file_format_version: 2,
                is_single_layer_and_tiled: false,
                has_long_names: false,
                has_deep_data: false,
                has_multiple_layers: false
            },
            headers: smallvec![ header ],
        };


        let mut data: Vec<u8> = Vec::new();
        meta.write_validating_to_buffered(&mut data, true).unwrap();
        let meta2 = MetaData::read_from_buffered(data.as_slice()).unwrap();
        meta2.validate(true).unwrap();
        assert_eq!(meta, meta2);
    }
}

