use wasm_bindgen::prelude::*;
use exr::prelude::*;
use exr::image::AnyChannels;
use std::io::Cursor;
use smallvec::smallvec;
use exr::image::pixel_vec::PixelVec;

/// Initialize panic hook for better error messages in browser console.
/// This is called automatically when the WASM module loads - no need to call manually.
#[wasm_bindgen(start)]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Represents a pre-built layer ready for encoding.
struct LayerData {
    name: Option<String>,
    channels: AnyChannels<FlatSamples>,
    compression: Compression,
}

/// Sample precision for pixel data.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default)]
pub enum SamplePrecision {
    /// 16-bit half float
    F16,
    /// 32-bit float (default)
    #[default]
    F32,
    /// 32-bit unsigned integer
    U32,
}

impl From<SamplePrecision> for exr::meta::attribute::SampleType {
    fn from(precision: SamplePrecision) -> Self {
        match precision {
            SamplePrecision::F16 => exr::meta::attribute::SampleType::F16,
            SamplePrecision::F32 => exr::meta::attribute::SampleType::F32,
            SamplePrecision::U32 => exr::meta::attribute::SampleType::U32,
        }
    }
}

/// Compression method for EXR output.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default)]
pub enum CompressionMethod {
    /// No compression - fastest, largest files
    None,
    /// Run-length encoding - fast, good for flat areas
    #[default]
    Rle,
    /// ZIP compression (single scanline) - slower, smaller files
    Zip,
    /// ZIP compression (16 scanlines) - good balance
    Zip16,
    /// PIZ wavelet compression - best for noisy images
    Piz,
    /// PXR24 - optimized for depth buffers (lossy for f32)
    Pxr24,
}

impl From<CompressionMethod> for Compression {
    fn from(method: CompressionMethod) -> Self {
        match method {
            CompressionMethod::None => Compression::Uncompressed,
            CompressionMethod::Rle => Compression::RLE,
            CompressionMethod::Zip => Compression::ZIP1,
            CompressionMethod::Zip16 => Compression::ZIP16,
            CompressionMethod::Piz => Compression::PIZ,
            CompressionMethod::Pxr24 => Compression::PXR24,
        }
    }
}

/// Encoder for creating multi-layer EXR images.
///
/// Use this class to construct EXR files with multiple AOV layers
/// (beauty, depth, normals, etc.) from WebGL/WebGPU render buffers.
#[wasm_bindgen]
pub struct ExrEncoder {
    width: usize,
    height: usize,
    layers: Vec<LayerData>,
}

#[wasm_bindgen]
impl ExrEncoder {
    /// Create a new EXR image builder.
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> ExrEncoder {
        ExrEncoder {
            width: width as usize,
            height: height as usize,
            layers: Vec::new(),
        }
    }

    /// Add a new layer with the specified channels.
    /// The `data` contains all pixels, each pixel with one float per channel.
    #[wasm_bindgen(js_name = addLayer)]
    pub fn add_layer(
        &mut self,
        name: Option<String>,
        channel_names: Vec<String>,
        interleaved: &[f32],
        precision: SamplePrecision,
        compression: CompressionMethod,
    ) -> std::result::Result<(), JsValue> {
        let pixel_count = self.width * self.height;
        let expected_len = pixel_count * channel_names.len();
        if interleaved.len() != expected_len {
            return Err(JsValue::from_str(&format!(
                "Layer '{}' expects {} floats ({}x{}x{}), got {}",
                name.unwrap_or_default(), expected_len, self.width, self.height, channel_names.len(), interleaved.len()
            )));
        }

        let sample_type: exr::meta::attribute::SampleType = precision.into();

        let any_channels = {
            if interleaved.len() == 1 {
                smallvec![Self::make_channel(&channel_names[0], interleaved.to_vec(), sample_type)]
            }
            else {
                let mut deinterleaved: Vec<Vec<f32>> = channel_names.iter().map(|_| Vec::with_capacity(pixel_count)).collect();

                for pixel in interleaved.chunks(deinterleaved.len()) {
                    for (chan, sample) in deinterleaved.iter_mut().zip(pixel) {
                        chan.push(*sample);
                    }
                }

                let any_channels: SmallVec<[_; 4]> = deinterleaved.into_iter()
                    .zip(channel_names)
                    .map(|(data, name)| Self::make_channel(&name, data, sample_type))
                    .collect();

                any_channels
            }
        };

        let channels = AnyChannels::sort(any_channels);

        self.layers.push(LayerData {
            name,
            channels,
            compression: compression.into(),
        });

        Ok(())
    }

    /// Encode the image to EXR bytes.
    ///
    /// Returns a Uint8Array containing the complete EXR file.
    #[wasm_bindgen(js_name = encode)]
    pub fn encode(&self) -> std::result::Result<Vec<u8>, JsValue> {
        if self.layers.is_empty() {
            return Err(JsValue::from_str("No layers added to image"));
        }

        self.build_and_encode()
            .map_err(|e| JsValue::from_str(&format!("EXR encoding error: {}", e)))
    }

    fn build_and_encode(&self) -> std::result::Result<Vec<u8>, exr::error::Error> {
        let size = Vec2(self.width, self.height);

        // Assemble layers from pre-built channel data
        let layers: smallvec::SmallVec<[Layer<AnyChannels<FlatSamples>>; 2]> = self
            .layers
            .iter()
            .map(|layer_data| {
                let encoding = Encoding {
                    compression: layer_data.compression,
                    blocks: Blocks::ScanLines,
                    line_order: LineOrder::Increasing,
                };

                Layer::new(
                    size,
                    LayerAttributes {
                        layer_name: layer_data.name.as_ref().map(|s| Text::new_or_none(s)).flatten(),
                        .. Default::default()
                    },
                    encoding,
                    layer_data.channels.clone(),
                )
            })
            .collect();

        // Create image with all layers
        let attributes = ImageAttributes::new(IntegerBounds::from_dimensions(size));
        let image: Image<smallvec::SmallVec<[Layer<AnyChannels<FlatSamples>>; 2]>> = Image {
            attributes,
            layer_data: layers,
        };

        // Write to in-memory buffer
        let mut buffer = Vec::new();
        {
            let cursor = Cursor::new(&mut buffer);
            image.write().non_parallel().to_buffered(cursor)?;
        }

        Ok(buffer)
    }

    fn make_channel(
        name: &str,
        data: Vec<f32>,
        sample_type: exr::meta::attribute::SampleType,
    ) -> AnyChannel<FlatSamples> {
        use exr::meta::attribute::SampleType;

        let samples = match sample_type {
            SampleType::F16 => {
                FlatSamples::F16(data.into_iter().map(|v| half::f16::from_f32(v)).collect())
            }
            SampleType::F32 => FlatSamples::F32(data),
            SampleType::U32 => {
                FlatSamples::U32(data.into_iter().map(|v| v as u32).collect())
            }
        };

        AnyChannel {
            name: Text::from(name),
            sample_data: samples,
            quantize_linearly: false,
            sampling: Vec2(1, 1),
        }
    }
}



/// Data for a single channel read from an EXR file.
struct ReadChannelData {
    name: String,
    samples: Vec<f32>,
}

/// Data for a single layer read from an EXR file.
struct ReadLayerData {
    name: Option<String>,
    channels: Vec<ReadChannelData>,
}

/// Decoder result from reading an EXR file.
///
/// Contains metadata and pixel data for all layers and channels.
#[wasm_bindgen]
pub struct ExrDecoder {
    width: u32,
    height: u32,
    layers: Vec<ReadLayerData>,
}

#[wasm_bindgen]
impl ExrDecoder {
    /// Image width in pixels.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Image height in pixels.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Number of layers in the image.
    #[wasm_bindgen(getter, js_name = layerCount)]
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Get the name of a layer by index.
    /// Returns null for the main/default layer (which has no name).
    #[wasm_bindgen(js_name = getLayerName)]
    pub fn get_layer_name(&self, index: usize) -> Option<String> {
        self.layers.get(index).and_then(|l| l.name.clone())
    }

    /// Get the channel names for a layer.
    #[wasm_bindgen(js_name = getLayerChannelNames)]
    pub fn get_layer_channel_names(&self, layer_index: usize) -> Vec<String> {
        self.layers
            .get(layer_index)
            .map(|l| l.channels.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Get interleaved pixel data for a layer.
    /// Returns null if any of the required channels are missing or if the layer index is invalid.
    /// Pixels are interleaved in the order specified by the provided channel names.
    #[wasm_bindgen(js_name = getLayerPixels)]
    pub fn get_layer_pixels(&self, layer_index: usize, channel_names: Vec<String>) -> Option<Vec<f32>> {
        let layer = self.layers.get(layer_index)?;

        let channels: Vec<_> = channel_names.iter()
            .flat_map(|name| layer.channels.iter().find(|c| &c.name == name))
            .collect();

        if channels.len() != channel_names.len() {
            return None;
        }

        if channels.len() == 1 {
            return Some(channels[0].samples.clone());
        }

        let pixel_count = (self.width * self.height) as usize;
        let mut interleaved = Vec::with_capacity(pixel_count * channels.len());

        for pixel_index in 0..pixel_count {
            for chan in &channels {
                interleaved.push(chan.samples[pixel_index]);
            }
        }

        Some(interleaved)
    }
}

/// Read an EXR file from bytes.
#[wasm_bindgen(js_name = readExr)]
pub fn read_exr(data: &[u8]) -> std::result::Result<ExrDecoder, JsValue> {
    read_exr_internal(data).map_err(|e| JsValue::from_str(&format!("EXR read error: {}", e)))
}

/// Read an EXR file expecting RGBA channels.
///
/// This is an optimized function that reads RGBA data directly into
/// interleaved format. Returns the first valid layer with RGBA channels.
#[wasm_bindgen(js_name = readExrRgba)]
pub fn read_exr_rgba(data: &[u8]) -> std::result::Result<ExrSimpleImage, JsValue> {
    use exr::prelude::*;

    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .rgba_channels(PixelVec::<(f32,f32,f32,f32)>::constructor, PixelVec::set_pixel)
        .first_valid_layer()
        .all_attributes()
        .from_buffered(Cursor::new(data))
        .map_err(|e| JsValue::from_str(&format!("EXR RGBA read error: {}", e)))?;

    Ok(ExrSimpleImage {
        width: image.layer_data.size.x() as u32,
        height: image.layer_data.size.y() as u32,
        data: image.layer_data.channel_data.pixels.pixels
            .iter().flat_map(|(r,g,b,a)| [r,g,b,a]).map(|f| *f).collect(), // TODO read into this structure directly to improve performance
    })
}

/// Result of optimized RGB(A) reading.
#[wasm_bindgen]
pub struct ExrSimpleImage {
    width: u32,
    height: u32,
    data: Vec<f32>,
}

#[wasm_bindgen]
impl ExrSimpleImage {
    /// Image width in pixels.
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Image height in pixels.
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Get the interleaved RGB pixel data as Float32Array.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Vec<f32> {
        self.data.clone()
    }
}

/// Read an EXR file expecting RGB channels.
///
/// This is an optimized function that reads RGB data directly into
/// interleaved format. Returns the first valid layer with RGB channels.
#[wasm_bindgen(js_name = readExrRgb)]
pub fn read_exr_rgb(data: &[u8]) -> std::result::Result<ExrSimpleImage, JsValue> {
    use exr::prelude::*;

    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .rgb_channels(PixelVec::<(f32,f32,f32)>::constructor, PixelVec::set_pixel)
        .first_valid_layer()
        .all_attributes()
        .from_buffered(Cursor::new(data))
        .map_err(|e| JsValue::from_str(&format!("EXR RGB read error: {}", e)))?;

    Ok(ExrSimpleImage {
        width: image.layer_data.size.x() as u32,
        height: image.layer_data.size.y() as u32,
        data: image.layer_data.channel_data.pixels.pixels
            .iter().flat_map(|(r,g,b)| [r,g,b]).map(|f| *f).collect(), // TODO read into this structure directly to improve performance
    })
}

/// Write a single RGBA layer to EXR bytes.
///
/// `data` must have length `width * height * 4`.
#[wasm_bindgen(js_name = writeExrRgba)]
pub fn write_exr_rgba(
    width: u32,
    height: u32,
    layer_name: Option<String>,
    data: &[f32],
    precision: SamplePrecision,
    compression: CompressionMethod,
) -> std::result::Result<Vec<u8>, JsValue> {
    let mut image = ExrEncoder::new(width, height);
    image.add_layer(layer_name, vec!["R".to_string(),"G".to_string(),"B".to_string(),"A".to_string()],
                    data, precision, compression)?;
    image.encode()
    // TODO: not use exrEncoder to improve performance and simplify the code
}

/// Write a single RGB layer to EXR bytes.
///
/// This is a convenience function for simple single-layer images.
/// No `.free()` call is needed - the result is returned directly.
///
/// # Arguments
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `layer_name` - Layer name (e.g., "normals")
/// * `data` - RGB pixel data as Float32Array, length must be width * height * 3
/// * `precision` - Sample precision (F16, F32, or U32)
/// * `compression` - Compression method
#[wasm_bindgen(js_name = writeExrRgb)]
pub fn write_exr_rgb(
    width: u32,
    height: u32,
    layer_name: Option<String>,
    data: &[f32],
    precision: SamplePrecision,
    compression: CompressionMethod,
) -> std::result::Result<Vec<u8>, JsValue> {
    let mut image = ExrEncoder::new(width, height);
    image.add_layer(layer_name, vec!["R".to_string(),"G".to_string(),"B".to_string()], data, precision, compression)?;
    image.encode()
    // TODO: not use exrEncoder to improve performance and simplify the code
}

/// Internal implementation of EXR reading (without JsValue for testing).
fn read_exr_internal(data: &[u8]) -> std::result::Result<ExrDecoder, exr::error::Error> {
    use exr::prelude::*;

    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .non_parallel()
        .from_buffered(Cursor::new(data))?;

    // Get dimensions from image attributes
    let bounds = image.attributes.display_window;
    let width = bounds.size.0 as u32;
    let height = bounds.size.1 as u32;

    // Convert layers
    let layers: Vec<ReadLayerData> = image
        .layer_data
        .iter()
        .map(|layer| {
            let name = layer.attributes.layer_name.as_ref().map(|n| n.to_string());

            let channels: Vec<ReadChannelData> = layer
                .channel_data
                .list
                .iter()
                .map(|channel| ReadChannelData {
                    name: channel.name.to_string(),
                    samples: channel.sample_data.values_as_f32().collect(),
                })
                .collect();

            ReadLayerData { name, channels }
        })
        .collect();

    Ok(ExrDecoder {
        width,
        height,
        layers,
    })
}
