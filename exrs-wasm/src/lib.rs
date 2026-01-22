use wasm_bindgen::prelude::*;
use exr::prelude::*;
use exr::image::AnyChannels;
use std::io::Cursor;

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

    /// Add an RGBA layer (4 channels: R, G, B, A).
    ///
    /// `data` must have length `width * height * 4`.
    #[wasm_bindgen(js_name = addRgbaLayer)]
    pub fn add_rgba_layer(
        &mut self,
        name: Option<String>,
        data: &[f32],
        precision: SamplePrecision,
        compression: Option<CompressionMethod>,
    ) -> std::result::Result<(), JsValue> {
        let pixel_count = self.width * self.height;
        let expected_len = pixel_count * 4;
        if data.len() != expected_len {
            return Err(JsValue::from_str(&format!(
                "RGBA layer '{}' expects {} floats ({}x{}x4), got {}",
                name.unwrap_or_default(), expected_len, self.width, self.height, data.len()
            )));
        }

        // Deinterleave and build channels
        let mut r = Vec::with_capacity(pixel_count);
        let mut g = Vec::with_capacity(pixel_count);
        let mut b = Vec::with_capacity(pixel_count);
        let mut a = Vec::with_capacity(pixel_count);

        for i in 0..pixel_count {
            r.push(data[i * 4]);
            g.push(data[i * 4 + 1]);
            b.push(data[i * 4 + 2]);
            a.push(data[i * 4 + 3]);
        }

        let sample_type: exr::meta::attribute::SampleType = precision.into();
        let channels = AnyChannels::sort(smallvec::smallvec![
            Self::make_channel("A", a, sample_type),
            Self::make_channel("B", b, sample_type),
            Self::make_channel("G", g, sample_type),
            Self::make_channel("R", r, sample_type),
        ]);

        self.layers.push(LayerData {
            name,
            channels,
            compression: compression.unwrap_or_default().into(),
        });

        Ok(())
    }

    /// Add an RGB layer (3 channels: R, G, B).
    ///
    /// `data` must have length `width * height * 3`.
    #[wasm_bindgen(js_name = addRgbLayer)]
    pub fn add_rgb_layer(
        &mut self,
        name: Option<String>,
        data: &[f32],
        precision: SamplePrecision,
        compression: Option<CompressionMethod>,
    ) -> std::result::Result<(), JsValue> {
        let pixel_count = self.width * self.height;
        let expected_len = pixel_count * 3;
        if data.len() != expected_len {
            return Err(JsValue::from_str(&format!(
                "RGB layer '{}' expects {} floats ({}x{}x3), got {}",
                name.unwrap_or_default(), expected_len, self.width, self.height, data.len()
            )));
        }

        // Deinterleave and build channels
        let mut r = Vec::with_capacity(pixel_count);
        let mut g = Vec::with_capacity(pixel_count);
        let mut b = Vec::with_capacity(pixel_count);

        for i in 0..pixel_count {
            r.push(data[i * 3]);
            g.push(data[i * 3 + 1]);
            b.push(data[i * 3 + 2]);
        }

        let sample_type: exr::meta::attribute::SampleType = precision.into();
        let channels = AnyChannels::sort(smallvec::smallvec![
            Self::make_channel("B", b, sample_type),
            Self::make_channel("G", g, sample_type),
            Self::make_channel("R", r, sample_type),
        ]);

        self.layers.push(LayerData {
            name,
            channels,
            compression: compression.unwrap_or_default().into(),
        });

        Ok(())
    }

    /// Add a single-channel layer with a custom channel name.
    ///
    /// `data` must have length `width * height`.
    #[wasm_bindgen(js_name = addSingleChannelLayer)]
    pub fn add_single_channel_layer(
        &mut self,
        name: Option<String>,
        channel_name: &str,
        data: &[f32],
        precision: SamplePrecision,
        compression: Option<CompressionMethod>,
    ) -> std::result::Result<(), JsValue> {
        let expected_len = self.width * self.height;
        if data.len() != expected_len {
            return Err(JsValue::from_str(&format!(
                "Single-channel layer '{}' expects {} floats ({}x{}), got {}",
                name.unwrap_or_default(), expected_len, self.width, self.height, data.len()
            )));
        }

        let sample_type: exr::meta::attribute::SampleType = precision.into();
        let channels = AnyChannels::sort(smallvec::smallvec![
            Self::make_channel(channel_name, data.to_vec(), sample_type),
        ]);

        self.layers.push(LayerData {
            name,
            channels,
            compression: compression.unwrap_or_default().into(),
        });

        Ok(())
    }

    /// Get the number of layers added so far.
    #[wasm_bindgen(getter, js_name = layerCount)]
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Encode the image to EXR bytes.
    ///
    /// Returns a Uint8Array containing the complete EXR file.
    #[wasm_bindgen(js_name = toBytes)]
    pub fn to_bytes(&self) -> std::result::Result<Vec<u8>, JsValue> {
        if self.layers.is_empty() {
            return Err(JsValue::from_str("No layers added to image"));
        }

        self.build_and_encode()
            .map_err(|e| JsValue::from_str(&format!("EXR encoding error: {}", e)))
    }

    /// Clear all layers (allows reusing the builder for a new frame).
    pub fn clear(&mut self) {
        self.layers.clear();
    }
}

// Private implementation
impl ExrEncoder {
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

// ============================================================================
// Reading EXR files
// ============================================================================

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
    #[wasm_bindgen(js_name = getChannelNames)]
    pub fn get_channel_names(&self, layer_index: usize) -> Vec<String> {
        self.layers
            .get(layer_index)
            .map(|l| l.channels.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Get the pixel data for a specific channel.
    /// Returns the data as Float32Array (all sample types converted to f32).
    #[wasm_bindgen(js_name = getChannelData)]
    pub fn get_channel_data(&self, layer_index: usize, channel_name: &str) -> Option<Vec<f32>> {
        self.layers.get(layer_index).and_then(|layer| {
            layer
                .channels
                .iter()
                .find(|c| c.name == channel_name)
                .map(|c| c.samples.clone())
        })
    }

    /// Get interleaved RGBA data for a layer (if R, G, B, A channels exist).
    /// Returns null if any of the required channels are missing.
    #[wasm_bindgen(js_name = getRgbaData)]
    pub fn get_rgba_data(&self, layer_index: usize) -> Option<Vec<f32>> {
        let layer = self.layers.get(layer_index)?;

        let r = layer.channels.iter().find(|c| c.name == "R")?;
        let g = layer.channels.iter().find(|c| c.name == "G")?;
        let b = layer.channels.iter().find(|c| c.name == "B")?;
        let a = layer.channels.iter().find(|c| c.name == "A")?;

        let pixel_count = r.samples.len();
        let mut result = Vec::with_capacity(pixel_count * 4);

        for i in 0..pixel_count {
            result.push(r.samples[i]);
            result.push(g.samples[i]);
            result.push(b.samples[i]);
            result.push(a.samples[i]);
        }

        Some(result)
    }

    /// Get interleaved RGB data for a layer (if R, G, B channels exist).
    /// Returns null if any of the required channels are missing.
    #[wasm_bindgen(js_name = getRgbData)]
    pub fn get_rgb_data(&self, layer_index: usize) -> Option<Vec<f32>> {
        let layer = self.layers.get(layer_index)?;

        let r = layer.channels.iter().find(|c| c.name == "R")?;
        let g = layer.channels.iter().find(|c| c.name == "G")?;
        let b = layer.channels.iter().find(|c| c.name == "B")?;

        let pixel_count = r.samples.len();
        let mut result = Vec::with_capacity(pixel_count * 3);

        for i in 0..pixel_count {
            result.push(r.samples[i]);
            result.push(g.samples[i]);
            result.push(b.samples[i]);
        }

        Some(result)
    }
}

/// Read an EXR file from bytes.
#[wasm_bindgen(js_name = readExr)]
pub fn read_exr(data: &[u8]) -> std::result::Result<ExrDecoder, JsValue> {
    read_exr_internal(data).map_err(|e| JsValue::from_str(&format!("EXR read error: {}", e)))
}

/// Result of optimized RGBA reading.
#[wasm_bindgen]
pub struct ExrRgbaResult {
    width: u32,
    height: u32,
    data: Vec<f32>,
}

#[wasm_bindgen]
impl ExrRgbaResult {
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

    /// Get the interleaved RGBA pixel data as Float32Array.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Vec<f32> {
        self.data.clone()
    }
}

/// Read an EXR file expecting RGBA channels.
///
/// This is an optimized function that reads RGBA data directly into
/// interleaved format. Returns the first valid layer with RGBA channels.
#[wasm_bindgen(js_name = readExrRgba)]
pub fn read_exr_rgba(data: &[u8]) -> std::result::Result<ExrRgbaResult, JsValue> {
    use exr::prelude::*;
    use std::cell::Cell;
    use std::rc::Rc;

    let img_width = Rc::new(Cell::new(0usize));
    let img_width_create = Rc::clone(&img_width);
    let img_width_set = Rc::clone(&img_width);

    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .rgba_channels(
            move |resolution, _channels| -> Vec<f32> {
                img_width_create.set(resolution.width());
                vec![0.0f32; resolution.width() * resolution.height() * 4]
            },
            move |pixels, position, (r, g, b, a): (f32, f32, f32, f32)| {
                let width = img_width_set.get();
                let idx = (position.y() * width + position.x()) * 4;
                pixels[idx] = r;
                pixels[idx + 1] = g;
                pixels[idx + 2] = b;
                pixels[idx + 3] = a;
            },
        )
        .first_valid_layer()
        .all_attributes()
        .non_parallel()
        .from_buffered(Cursor::new(data))
        .map_err(|e| JsValue::from_str(&format!("EXR RGBA read error: {}", e)))?;

    let bounds = image.attributes.display_window;
    let width = bounds.size.0 as u32;
    let height = bounds.size.1 as u32;

    Ok(ExrRgbaResult {
        width,
        height,
        data: image.layer_data.channel_data.pixels,
    })
}

/// Result of optimized RGB reading.
#[wasm_bindgen]
pub struct ExrRgbResult {
    width: u32,
    height: u32,
    data: Vec<f32>,
}

#[wasm_bindgen]
impl ExrRgbResult {
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
pub fn read_exr_rgb(data: &[u8]) -> std::result::Result<ExrRgbResult, JsValue> {
    use exr::prelude::*;
    use std::cell::Cell;
    use std::rc::Rc;

    let img_width = Rc::new(Cell::new(0usize));
    let img_width_create = Rc::clone(&img_width);
    let img_width_set = Rc::clone(&img_width);

    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .rgb_channels(
            move |resolution, _channels| -> Vec<f32> {
                img_width_create.set(resolution.width());
                vec![0.0f32; resolution.width() * resolution.height() * 3]
            },
            move |pixels, position, (r, g, b): (f32, f32, f32)| {
                let width = img_width_set.get();
                let idx = (position.y() * width + position.x()) * 3;
                pixels[idx] = r;
                pixels[idx + 1] = g;
                pixels[idx + 2] = b;
            },
        )
        .first_valid_layer()
        .all_attributes()
        .non_parallel()
        .from_buffered(Cursor::new(data))
        .map_err(|e| JsValue::from_str(&format!("EXR RGB read error: {}", e)))?;

    let bounds = image.attributes.display_window;
    let width = bounds.size.0 as u32;
    let height = bounds.size.1 as u32;

    Ok(ExrRgbResult {
        width,
        height,
        data: image.layer_data.channel_data.pixels,
    })
}

// ============================================================================
// Convenience functions (no .free() needed)
// ============================================================================

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
    image.add_rgba_layer(layer_name, data, precision, Some(compression))?;
    image.to_bytes()
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
    image.add_rgb_layer(layer_name, data, precision, Some(compression))?;
    image.to_bytes()
}

/// Write a single-channel layer to EXR bytes.
///
/// This is a convenience function for simple single-layer images (e.g., depth maps).
/// No `.free()` call is needed - the result is returned directly.
///
/// # Arguments
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `layer_name` - Layer name (e.g., "depth")
/// * `channel_name` - Channel name (e.g., "Z" for depth)
/// * `data` - Pixel data as Float32Array, length must be width * height
/// * `precision` - Sample precision (F16, F32, or U32)
/// * `compression` - Compression method
#[wasm_bindgen(js_name = writeExrSingleChannel)]
pub fn write_exr_single_channel(
    width: u32,
    height: u32,
    layer_name: Option<String>,
    channel_name: &str,
    data: &[f32],
    precision: SamplePrecision,
    compression: CompressionMethod,
) -> std::result::Result<Vec<u8>, JsValue> {
    let mut image = ExrEncoder::new(width, height);
    image.add_single_channel_layer(layer_name, channel_name, data, precision, Some(compression))?;
    image.to_bytes()
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
