
//! Contains collections of common attributes.
//! Defines some data types that list all standard attributes.

use std::collections::HashMap;
use crate::meta::attribute::*; // FIXME shouldn't this need some more imports????
use crate::meta::*;


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
/// For more attributes, see struct `LayerAttributes`.
#[derive(Clone, PartialEq, Debug)]
pub struct ImageAttributes {

    /// The rectangle anywhere in the global infinite 2D space
    /// that clips all contents of the file.
    pub display_window: IntRect,

    /// Aspect ratio of each pixel in this header.
    pub pixel_aspect: f32,

    /// The chromaticities attribute of the image. See the `Chromaticities` type.
    pub chromaticities: Option<Chromaticities>,

    /// The time code of the image.
    pub time_code: Option<TimeCode>,

    /// Optional attributes. Contains custom attributes.
    /// Does not contain the attributes already present in the `ImageAttributes`.
    /// Contains only attributes that are standardized to be the same for all headers: chromaticities and time codes.
    pub custom: HashMap<Text, AttributeValue>,
}

/// Does not include the attributes required for reading the file contents.
/// Excludes standard fields that must be the same for all headers.
/// For more attributes, see struct `ImageAttributes`.
#[derive(Clone, PartialEq)]
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

    // TODO same for all layers?
    /// Part of the perspective projection. Default should be `1`.
    pub screen_window_width: f32, // TODO integrate into `list`

    /// The white luminance of the colors.
    /// Defines the luminance in candelas per square meter, Nits, of the RGB value `(1, 1, 1)`.
    // If the chromaticities and the whiteLuminance of an RGB image are
    // known, then it is possible to convert the image's pixels from RGB
    // to CIE XYZ tristimulus values (see function RGBtoXYZ() in header
    // file ImfChromaticities.h).
    pub white_luminance: Option<f32>,

    /// The adopted neutral of the colors. Specifies the CIE (x,y) frequency coordinates that should
    /// be considered neutral during color rendering. Pixels in the image
    /// whose CIE (x,y) frequency coordinates match the adopted neutral value should
    /// be mapped to neutral values on the given display.
    pub adopted_neutral: Option<Vec2<f32>>,

    /// Name of the color transform function that is applied for rendering the image.
    pub rendering_transform: Option<Text>,

    /// Name of the color transform function that computes the look modification of the image.
    pub look_modification_transform: Option<Text>,

    /// The horizontal density, in pixels per inch.
    /// The image's vertical output density can be computed using `x_density * pixel_aspect_ratio`.
    pub x_density: Option<f32>,

    /// Name of the owner.
    pub owner: Option<Text>,

    /// Additional textual information.
    pub comments: Option<Text>,

    /// The date of image creation, in `YYYY:MM:DD hh:mm:ss` format.
    // TODO parse!
    pub capture_date: Option<Text>,

    /// Time offset from UTC.
    pub utc_offset: Option<f32>,

    /// Geographical image location.
    pub longitude: Option<f32>,

    /// Geographical image location.
    pub latitude: Option<f32>,

    /// Geographical image location.
    pub altitude: Option<f32>,

    /// Camera focus in meters.
    pub focus: Option<f32>,

    /// Exposure time in seconds.
    pub exposure: Option<f32>,

    /// Camera aperture measured in f-stops. Equals the focal length
    /// of the lens divided by the diameter of the iris opening.
    pub aperture: Option<f32>,

    /// Iso-speed of the camera sensor.
    pub iso_speed: Option<f32>,

    /// If this is an environment map, specifies how to interpret it.
    pub environment_map: Option<EnvironmentMap>,

    /// Identifies film manufacturer, film type, film roll and frame position within the roll.
    pub key_code: Option<KeyCode>,

    /// Specifies how texture map images are extrapolated.
    /// Values can be `black`, `clamp`, `periodic`, or `mirror`.
    pub wrap_modes: Option<Text>,

    /// Frames per second if this is a frame in a sequence.
    pub frames_per_second: Option<Rational>,

    /// Specifies the view names for multi-view, for example stereo, image files.
    pub multi_view: Option<Vec<Text>>,

    /// The matrix that transforms 3D points from the world to the camera coordinate space.
    /// Left-handed coordinate system, y up, z forward.
    pub world_to_camera: Option<Matrix4x4>,

    /// The matrix that transforms 3D points from the world to the "Normalized Device Coordinate" space.
    /// Left-handed coordinate system, y up, z forward.
    pub world_to_normalized_device: Option<Matrix4x4>,

    /// Specifies whether the pixels in a deep image are sorted and non-overlapping.
    pub deep_image_state: Option<Rational>,

    /// If the image was cropped, contains the original data window.
    pub original_data_window: Option<IntRect>,

    /// Level of compression in DWA images.
    pub dwa_compression_level: Option<f32>,

    /// An 8-bit RGBA image representing the rendered image.
    pub preview: Option<Preview>,

    /// Name of the view, which is probably either `"right"` or `"left"` for a stereoscopic image.
    pub view: Option<Text>,

    /// Optional attributes. Contains custom attributes.
    /// Does not contain the attributes already present in the `Header` or `LayerAttributes` struct.
    /// Does not contain attributes that are standardized to be the same for all layers: no chromaticities and no time codes.
    pub custom: HashMap<Text, AttributeValue>,
}


impl LayerAttributes {

    /// Create default layer attributes with a data position of zero.
    pub fn new(layer_name: Text) -> Self {
        Self {
            name: Some(layer_name),
            .. Self::default()
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
    pub fn new(display_size: impl Into<Vec2<usize>>) -> Self {
        Self {
            display_window: IntRect::from_dimensions(display_size),
            .. Self::default()
        }
    }

    /// Set the data position of this layer.
    pub fn with_display_window(self, display_window: IntRect) -> Self {
        Self { display_window, ..self }
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
    pub fn new(name: Text, data_size: impl Into<Vec2<usize>>, channels: SmallVec<[ChannelInfo; 5]>) -> Self {
        let data_size: Vec2<usize> = data_size.into();
        let compression = Compression::Uncompressed;
        let blocks = Blocks::ScanLines;

        Self {
            data_size,
            compression,
            blocks,

            channels: ChannelList::new(channels),
            line_order: LineOrder::Unspecified,

            shared_attributes: ImageAttributes::new(data_size),
            own_attributes: LayerAttributes::new(name),

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

            divide_and_rest(image_size.height(), tile_size.height()).flat_map(move |(y_index, tile_height)|{
                divide_and_rest(image_size.width(), tile_size.width()).map(move |(x_index, tile_width)|{
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

            let data_width = compute_level_size(tiles.rounding_mode, data_width, tile.level_index.x());
            let data_height = compute_level_size(tiles.rounding_mode, data_height, tile.level_index.y());
            let absolute_tile_coordinates = tile.to_data_indices(tiles.tile_size, Vec2(data_width, data_height))?;

            if absolute_tile_coordinates.position.x() as i64 >= data_width as i64 || absolute_tile_coordinates.position.y() as i64 >= data_height as i64 {
                return Err(Error::invalid("data block tile index"))
            }

            absolute_tile_coordinates
        }
        else { // this is a scanline image
            debug_assert_eq!(tile.tile_index.0, 0, "block index calculation bug");

            let (y, height) = calculate_block_position_and_size(
                self.data_size.height(),
                self.compression.scan_lines_per_block(),
                tile.tile_index.y()
            )?;

            IntRect {
                position: Vec2(0, usize_to_i32(y)),
                size: Vec2(self.data_size.width(), height)
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
                let y = (block.y_coordinate - self.own_attributes.data_position.y()) / size;

                if y < 0 {
                    return Err(Error::invalid("scan block y coordinate"));
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
            Blocks::Tiles(tiles) => tiles.tile_size.area(),
            Blocks::ScanLines => self.compression.scan_lines_per_block() * self.data_size.width()
            // TODO What about deep data???
        }
    }

    /// Validate this instance.
    pub fn validate(&self, is_multilayer: bool, long_names: &mut bool, strict: bool) -> UnitResult {
        debug_assert_eq!(
            self.chunk_count, compute_chunk_count(self.compression, self.data_size, self.blocks),
            "incorrect chunk count value"
        );

        self.data_window().validate(None)?;
        self.shared_attributes.display_window.validate(None)?;

        if strict {
            if is_multilayer {
                if self.own_attributes.name.is_none() {
                    return Err(missing_attribute("layer name for multi layer file"));
                }
            }

            if self.blocks == Blocks::ScanLines && self.line_order == LineOrder::Unspecified {
                return Err(Error::invalid("unspecified line order in scan line images"));
            }

            if self.data_size == Vec2(0,0) {
                return Err(Error::invalid("empty data window"));
            }

            if self.shared_attributes.display_window.size == Vec2(0,0) {
                return Err(Error::invalid("empty display window"));
            }

            if !self.shared_attributes.pixel_aspect.is_normal() || self.shared_attributes.pixel_aspect < 1.0e-6 || self.shared_attributes.pixel_aspect > 1.0e6 {
                return Err(Error::invalid("pixel aspect ratio"));
            }

            if self.own_attributes.screen_window_width < 0.0 {
                return Err(Error::invalid("screen window width"));
            }
        }


        let allow_subsampling = !self.deep && self.blocks == Blocks::ScanLines;
        self.channels.validate(allow_subsampling, self.data_window(), strict)?;

        for (name, value) in &self.shared_attributes.custom {
            attribute::validate(name, value, long_names, allow_subsampling, self.data_window(), strict)?;
        }

        for (name, value) in &self.own_attributes.custom {
            attribute::validate(name, value, long_names, allow_subsampling, self.data_window(), strict)?;
        }


        // check if attribute names appear twice
        if strict {
            for (name, _) in &self.shared_attributes.custom {
                if !self.own_attributes.custom.contains_key(&name) {
                    return Err(Error::invalid(format!("duplicate attribute name: `{}`", name)));
                }
            }

            for &reserved in header::standard_names::ALL.iter() {
                let name  = Text::from_bytes_unchecked(SmallVec::from_slice(reserved));
                if self.own_attributes.custom.contains_key(&name) || self.shared_attributes.custom.contains_key(&name) {
                    return Err(Error::invalid(format!(
                        "attribute name `{}` is reserved and cannot be custom",
                        Text::from_bytes_unchecked(reserved.into())
                    )));
                }
            }
        }

        if self.deep {
            if strict {
                if self.own_attributes.name.is_none() {
                    return Err(missing_attribute("layer name for deep file"));
                }

                if self.max_samples_per_pixel.is_none() {
                    return Err(Error::invalid("missing max samples per pixel attribute for deepdata"));
                }
            }

            match self.deep_data_version {
                Some(1) => {},
                Some(_) => return Err(Error::unsupported("deep data version")),
                None => return Err(missing_attribute("deep data version")),
            }

            if !self.compression.supports_deep_data() {
                return Err(Error::invalid("compression method does not support deep data"));
            }
        }

        Ok(())
    }

    /// Read the headers without validating them.
    pub fn read_all(read: &mut PeekRead<impl Read>, version: &Requirements, skip_invalid_attributes: bool) -> Result<Headers> {
        if !version.is_multilayer() {
            Ok(smallvec![ Header::read(read, version, skip_invalid_attributes)? ])
        }
        else {
            let mut headers = SmallVec::new();

            while !sequence_end::has_come(read)? {
                headers.push(Header::read(read, version, skip_invalid_attributes)?);
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
    pub fn read(read: &mut PeekRead<impl Read>, requirements: &Requirements, skip_invalid_attributes: bool) -> Result<Self> {
        let max_string_len = if requirements.has_long_names { 256 } else { 32 }; // TODO DRY this information

        // these required attributes will be filled when encountered while parsing
        let mut tiles = None;
        let mut block_type = None;
        let mut version = None;
        let mut chunk_count = None;
        let mut max_samples_per_pixel = None;
        let mut channels = None;
        let mut compression = None;
        let mut data_window = None;
        let mut display_window = None;
        let mut line_order = None;
        let mut layer_attributes = LayerAttributes::default();
        let mut image_attributes = ImageAttributes::default();

        // read each attribute in this header
        while !sequence_end::has_come(read)? {
            let (attribute_name, value) = attribute::read(read, max_string_len)?;

            // if the attribute value itself is ok, record it
            match value {
                Ok(value) => {
                    use crate::meta::header::standard_names as name;
                    use crate::meta::attribute::AttributeValue::*;

                    // if the attribute is a required attribute, set the corresponding variable directly.
                    // otherwise, add the attribute to the vector of custom attributes

                    // the following attributes will only be set if the type matches the commonly used type for that attribute
                    match (attribute_name.bytes(), value) {
                        (name::BLOCK_TYPE, Text(value)) => block_type = Some(attribute::BlockType::parse(value)?),
                        (name::TILES, TileDescription(value)) => tiles = Some(value),
                        (name::CHANNELS, ChannelList(value)) => channels = Some(value),
                        (name::COMPRESSION, Compression(value)) => compression = Some(value),
                        (name::DATA_WINDOW, IntRect(value)) => data_window = Some(value),
                        (name::DISPLAY_WINDOW, IntRect(value)) => display_window = Some(value),
                        (name::LINE_ORDER, LineOrder(value)) => line_order = Some(value),
                        (name::DEEP_DATA_VERSION, I32(value)) => version = Some(value),

                        (name::MAX_SAMPLES, I32(value)) => max_samples_per_pixel = Some(
                            i32_to_usize(value, "max sample count")?
                        ),

                        (name::CHUNKS, I32(value)) => chunk_count = Some(
                            i32_to_usize(value, "chunk count")?
                        ),

                        (name::NAME, Text(value)) => layer_attributes.name = Some(value),
                        (name::WINDOW_CENTER, FloatVec2(value)) => layer_attributes.screen_window_center = value,
                        (name::WINDOW_WIDTH, F32(value)) => layer_attributes.screen_window_width = value,

                        (name::WHITE_LUMINANCE, F32(value)) => layer_attributes.white_luminance = Some(value),
                        (name::ADOPTED_NEUTRAL, FloatVec2(value)) => layer_attributes.adopted_neutral = Some(value),
                        (name::RENDERING_TRANSFORM, Text(value)) => layer_attributes.rendering_transform = Some(value),
                        (name::LOOK_MOD_TRANSFORM, Text(value)) => layer_attributes.look_modification_transform = Some(value),
                        (name::X_DENSITY, F32(value)) => layer_attributes.x_density = Some(value),

                        (name::OWNER, Text(value)) => layer_attributes.owner = Some(value),
                        (name::COMMENTS, Text(value)) => layer_attributes.comments = Some(value),
                        (name::CAPTURE_DATE, Text(value)) => layer_attributes.capture_date = Some(value),
                        (name::UTC_OFFSET, F32(value)) => layer_attributes.utc_offset = Some(value),
                        (name::LONGITUDE, F32(value)) => layer_attributes.longitude = Some(value),
                        (name::LATITUDE, F32(value)) => layer_attributes.latitude = Some(value),
                        (name::ALTITUDE, F32(value)) => layer_attributes.altitude = Some(value),
                        (name::FOCUS, F32(value)) => layer_attributes.focus = Some(value),
                        (name::EXPOSURE_TIME, F32(value)) => layer_attributes.exposure = Some(value),
                        (name::APERTURE, F32(value)) => layer_attributes.aperture = Some(value),
                        (name::ISO_SPEED, F32(value)) => layer_attributes.iso_speed = Some(value),
                        (name::ENVIRONMENT_MAP, EnvironmentMap(value)) => layer_attributes.environment_map = Some(value),
                        (name::KEY_CODE, KeyCode(value)) => layer_attributes.key_code = Some(value),
                        (name::WRAP_MODES, Text(value)) => layer_attributes.wrap_modes = Some(value),
                        (name::FRAMES_PER_SECOND, Rational(value)) => layer_attributes.frames_per_second = Some(value),
                        (name::MULTI_VIEW, TextVector(value)) => layer_attributes.multi_view = Some(value),
                        (name::WORLD_TO_CAMERA, Matrix4x4(value)) => layer_attributes.world_to_camera = Some(value),
                        (name::WORLD_TO_NDC, Matrix4x4(value)) => layer_attributes.world_to_normalized_device = Some(value),
                        (name::DEEP_IMAGE_STATE, Rational(value)) => layer_attributes.deep_image_state = Some(value),
                        (name::ORIGINAL_DATA_WINDOW, IntRect(value)) => layer_attributes.original_data_window = Some(value),
                        (name::DWA_COMPRESSION_LEVEL, F32(value)) => layer_attributes.dwa_compression_level = Some(value),
                        (name::PREVIEW, Preview(value)) => layer_attributes.preview = Some(value),
                        (name::VIEW, Text(value)) => layer_attributes.view = Some(value),

                        (name::PIXEL_ASPECT, F32(value)) => image_attributes.pixel_aspect = value,
                        (name::TIME_CODE, TimeCode(value)) => image_attributes.time_code = Some(value),
                        (name::CHROMATICITIES, Chromaticities(value)) => image_attributes.chromaticities = Some(value),

                        // insert unknown attributes of these types into image attributes,
                        // as these must be the same for all headers
                        (_, value @ Chromaticities(_)) |
                        (_, value @ TimeCode(_)) => {
                            image_attributes.custom.insert(attribute_name, value);
                        },

                        // insert unknown attributes into layer attributes
                        (_, value) => {
                            layer_attributes.custom.insert(attribute_name, value);
                        },

                    }
                },

                // in case the attribute value itself is not ok, but the rest of the image is
                // only abort reading the image if desired
                Err(error) => {
                    if !skip_invalid_attributes { return Err(error); }
                }
            }
        }

        let compression = compression.ok_or(missing_attribute("compression"))?;
        let data_window = data_window.ok_or(missing_attribute("data window"))?;

        image_attributes.display_window = display_window.ok_or(missing_attribute("display window"))?;
        layer_attributes.data_position = data_window.position;

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

        // check size now to prevent panics while computing the chunk size
        data_window.validate(None)?;

        let computed_chunk_count = compute_chunk_count(compression, data_size, blocks);
        if chunk_count.is_some() && chunk_count != Some(computed_chunk_count) {
            return Err(Error::invalid("chunk count not matching data size"));
        }

        let header = Header {
            compression,

            // always compute ourselves, because we cannot trust anyone out there 😱
            chunk_count: computed_chunk_count,

            data_size,

            shared_attributes: image_attributes,
            own_attributes: layer_attributes,

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

        macro_rules! write_attributes {
            ( $($name: ident : $variant: ident = $value: expr),* ) => { $(
                attribute::write($name, & $variant ($value .clone()), write)?; // TODO without clone
            )* };
        }

        macro_rules! write_optional_attributes {
            ( $($name: ident : $variant: ident = $value: expr),* ) => { $(
                if let Some(value) = $value {
                    attribute::write($name, & $variant (value.clone()), write)?; // TODO without clone
                };
            )* };
        }

        {
            use crate::meta::header::standard_names::*;
            use AttributeValue::*;

            let (block_type, tiles) = match self.blocks {
                Blocks::ScanLines => (attribute::BlockType::ScanLine, None),
                Blocks::Tiles(tiles) => (attribute::BlockType::Tile, Some(tiles))
            };

            fn usize_as_i32(value: usize) -> AttributeValue {
                I32(i32::try_from(value).expect("u32 exceeds i32 range"))
            }

            write_optional_attributes!(
                TILES: TileDescription = &tiles,
                DEEP_DATA_VERSION: I32 = &self.deep_data_version,
                MAX_SAMPLES: usize_as_i32 = &self.max_samples_per_pixel
            );

            write_attributes!(
                // chunks is not actually required, but always computed in this library anyways
                CHUNKS: usize_as_i32 = &self.chunk_count,

                BLOCK_TYPE: BlockType = &block_type,
                CHANNELS: ChannelList = &self.channels,
                COMPRESSION: Compression = &self.compression,
                LINE_ORDER: LineOrder = &self.line_order,
                DATA_WINDOW: IntRect = &self.data_window(),

                DISPLAY_WINDOW: IntRect = &self.shared_attributes.display_window,
                PIXEL_ASPECT: F32 = &self.shared_attributes.pixel_aspect,

                WINDOW_CENTER: FloatVec2 = &self.own_attributes.screen_window_center,
                WINDOW_WIDTH: F32 = &self.own_attributes.screen_window_width
            );

            write_optional_attributes!(
                NAME: Text = &self.own_attributes.name,
                WHITE_LUMINANCE: F32 = &self.own_attributes.white_luminance,
                ADOPTED_NEUTRAL: FloatVec2 = &self.own_attributes.adopted_neutral,
                RENDERING_TRANSFORM: Text = &self.own_attributes.rendering_transform,
                LOOK_MOD_TRANSFORM: Text = &self.own_attributes.look_modification_transform,
                X_DENSITY: F32 = &self.own_attributes.x_density,
                OWNER: Text = &self.own_attributes.owner,
                COMMENTS: Text = &self.own_attributes.comments,
                CAPTURE_DATE: Text = &self.own_attributes.capture_date,
                UTC_OFFSET: F32 = &self.own_attributes.utc_offset,
                LONGITUDE: F32 = &self.own_attributes.longitude,
                LATITUDE: F32 = &self.own_attributes.latitude,
                ALTITUDE: F32 = &self.own_attributes.altitude,
                FOCUS: F32 = &self.own_attributes.focus,
                EXPOSURE_TIME: F32 = &self.own_attributes.exposure,
                APERTURE: F32 = &self.own_attributes.aperture,
                ISO_SPEED: F32 = &self.own_attributes.iso_speed,
                ENVIRONMENT_MAP: EnvironmentMap = &self.own_attributes.environment_map,
                KEY_CODE: KeyCode = &self.own_attributes.key_code,
                TIME_CODE: TimeCode = &self.shared_attributes.time_code,
                WRAP_MODES: Text = &self.own_attributes.wrap_modes,
                FRAMES_PER_SECOND: Rational = &self.own_attributes.frames_per_second,
                MULTI_VIEW: TextVector = &self.own_attributes.multi_view,
                WORLD_TO_CAMERA: Matrix4x4 = &self.own_attributes.world_to_camera,
                WORLD_TO_NDC: Matrix4x4 = &self.own_attributes.world_to_normalized_device,
                DEEP_IMAGE_STATE: Rational = &self.own_attributes.deep_image_state,
                ORIGINAL_DATA_WINDOW: IntRect = &self.own_attributes.original_data_window,
                DWA_COMPRESSION_LEVEL: F32 = &self.own_attributes.dwa_compression_level,
                CHROMATICITIES: Chromaticities = &self.shared_attributes.chromaticities,
                PREVIEW: Preview = &self.own_attributes.preview,
                VIEW: Text = &self.own_attributes.view
            );
        }

        for (name, value) in &self.shared_attributes.custom {
            attribute::write(name.bytes(), value, write)?;
        }

        for (name, value) in &self.own_attributes.custom {
            attribute::write(name.bytes(), value, write)?;
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



/// Collection of required attribute names.
pub mod standard_names {
    macro_rules! define_required_attribute_names {
        ( $($name: ident  :  $value: expr),* ) => {

            /// A list containing all reserved names.
            pub const ALL: &'static [&'static [u8]] = &[
                $( $value ),*
            ];

            $(
                /// The byte-string name of this required attribute as it appears in an exr file.
                pub const $name: &'static [u8] = $value;
            )*
        };
    }

    define_required_attribute_names! {
        TILES: b"tiles",
        NAME: b"name",
        BLOCK_TYPE: b"type",
        DEEP_DATA_VERSION: b"version",
        CHUNKS: b"chunkCount",
        MAX_SAMPLES: b"maxSamplesPerPixel",
        CHANNELS: b"channels",
        COMPRESSION: b"compression",
        DATA_WINDOW: b"dataWindow",
        DISPLAY_WINDOW: b"displayWindow",
        LINE_ORDER: b"lineOrder",
        PIXEL_ASPECT: b"pixelAspectRatio",
        WINDOW_CENTER: b"screenWindowCenter",
        WINDOW_WIDTH: b"screenWindowWidth",
        WHITE_LUMINANCE: b"whiteLuminance",
        ADOPTED_NEUTRAL: b"adoptedNeutral",
        RENDERING_TRANSFORM: b"renderingTransform",
        LOOK_MOD_TRANSFORM: b"lookModTransform",
        X_DENSITY: b"xDensity",
        OWNER: b"owner",
        COMMENTS: b"comments",
        CAPTURE_DATE: b"capDate",
        UTC_OFFSET: b"utcOffset",
        LONGITUDE: b"longitude",
        LATITUDE: b"latitude",
        ALTITUDE: b"altitude",
        FOCUS: b"focus",
        EXPOSURE_TIME: b"expTime",
        APERTURE: b"aperture",
        ISO_SPEED: b"isoSpeed",
        ENVIRONMENT_MAP: b"envmap",
        KEY_CODE: b"keyCode",
        TIME_CODE: b"timeCode",
        WRAP_MODES: b"wrapmodes",
        FRAMES_PER_SECOND: b"framesPerSecond",
        MULTI_VIEW: b"multiView",
        WORLD_TO_CAMERA: b"worldToCamera",
        WORLD_TO_NDC: b"worldToNDC",
        DEEP_IMAGE_STATE: b"deepImageState",
        ORIGINAL_DATA_WINDOW: b"originalDataWindow",
        DWA_COMPRESSION_LEVEL: b"dwaCompressionLevel",
        PREVIEW: b"preview",
        VIEW: b"view",
        CHROMATICITIES: b"chromaticities"
    }
}


impl Default for LayerAttributes {
    fn default() -> Self {
        Self {
            data_position: Vec2(0, 0),
            screen_window_center: Vec2(0.0, 0.0),
            screen_window_width: 1.0,
            name: None,
            white_luminance: None,
            adopted_neutral: None,
            rendering_transform: None,
            look_modification_transform: None,
            x_density: None,
            owner: None,
            comments: None,
            capture_date: None,
            utc_offset: None,
            longitude: None,
            latitude: None,
            altitude: None,
            focus: None,
            exposure: None,
            aperture: None,
            iso_speed: None,
            environment_map: None,
            key_code: None,
            wrap_modes: None,
            frames_per_second: None,
            multi_view: None,
            world_to_camera: None,
            world_to_normalized_device: None,
            deep_image_state: None,
            original_data_window: None,
            dwa_compression_level: None,
            preview: None,
            view: None,
            custom: Default::default()
        }
    }
}

impl std::fmt::Debug for LayerAttributes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let default_self = Self::default();

        let mut debug = formatter.debug_struct("LayerAttributes (only relevant attributes)");

        // always debug the following fields
        debug.field("data_position", &self.data_position);
        debug.field("name", &self.name);

        macro_rules! debug_non_default_fields {
            ( $( $name: ident ),* ) => { $(

                if self.$name != default_self.$name {
                    debug.field(stringify!($name), &self.$name);
                }

            )* };
        }

        // only debug these fields if they are not the default value
        debug_non_default_fields! {
            screen_window_center, screen_window_width,
            white_luminance, adopted_neutral, x_density,
            rendering_transform, look_modification_transform,
            owner, comments,
            capture_date, utc_offset,
            longitude, latitude, altitude,
            focus, exposure, aperture, iso_speed,
            environment_map, key_code, wrap_modes,
            frames_per_second, multi_view,
            world_to_camera, world_to_normalized_device,
            deep_image_state, original_data_window,
            dwa_compression_level,
            preview, view,
            custom
        }

        debug.finish()
    }
}

impl Default for ImageAttributes {
    fn default() -> Self {
        Self {
            pixel_aspect: 1.0,
            chromaticities: None,
            time_code: None,
            custom: Default::default(),
            display_window: Default::default(),
        }
    }
}