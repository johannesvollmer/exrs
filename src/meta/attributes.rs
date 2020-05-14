
//! Contains collections of common attributes.
//! Defines some data types that list all standard attributes.

use std::collections::HashMap;
use crate::prelude::common::*;
use attribute::*;


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