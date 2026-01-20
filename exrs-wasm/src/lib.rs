//! WebAssembly bindings for reading and writing EXR files in the browser.
//!
//! This crate provides JavaScript-friendly APIs for creating and reading multi-layer
//! EXR files with AOVs (Arbitrary Output Variables) commonly used in rendering pipelines.
//!
//! # Simple Writing Example (JavaScript)
//!
//! For simple single-layer images, use the functional API (no `.free()` needed):
//!
//! ```javascript
//! import init, { writeExrRgba, SamplePrecision, CompressionMethod } from 'exrs-wasm';
//!
//! await init();
//!
//! const bytes = writeExrRgba(1920, 1080, 'beauty', beautyPixels,
//!                            SamplePrecision.F32, CompressionMethod.Piz);
//! // Download or save bytes...
//! ```
//!
//! # Multi-Layer Writing Example (JavaScript)
//!
//! For multi-layer images, use the builder API:
//!
//! ```javascript
//! import init, { ExrEncoder, CompressionMethod, SamplePrecision } from 'exrs-wasm';
//!
//! await init();
//!
//! const exr = new ExrEncoder(1920, 1080);
//! exr.addRgbaLayer('beauty', beautyPixels, SamplePrecision.F32, CompressionMethod.Piz);
//! exr.addSingleChannelLayer('depth', 'Z', depthPixels, SamplePrecision.F32, CompressionMethod.Pxr24);
//! exr.addRgbLayer('normals', normalPixels, SamplePrecision.F16, CompressionMethod.Zip16);
//!
//! const bytes = exr.toBytes();
//! // exr.free() is optional - memory is automatically freed when GC runs
//! ```
//!
//! # Reading Example (JavaScript)
//!
//! ```javascript
//! import init, { readExr } from 'exrs-wasm';
//!
//! await init();
//!
//! const result = readExr(exrBytes);
//! console.log('Dimensions:', result.width, 'x', result.height);
//! console.log('Layers:', result.layerCount);
//!
//! // Get RGBA data for first layer (if it has R, G, B, A channels)
//! const rgbaData = result.getRgbaData(0);
//!
//! // Or get individual channel data
//! const depthData = result.getChannelData(1, 'Z');
//! ```

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

/// Represents pixel data for a single layer.
struct LayerData {
    name: String,
    channel_type: ChannelType,
    pixels: Vec<f64>,
    sample_type: exr::meta::attribute::SampleType,
    compression: Compression,
}

/// The type of channels in a layer.
#[derive(Clone)]
enum ChannelType {
    /// RGBA - 4 channels (R, G, B, A)
    Rgba,
    /// RGB - 3 channels (R, G, B)
    Rgb,
    /// Single channel with custom name (e.g., "Z" for depth)
    Single(String),
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
    ///
    /// # Arguments
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
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
    /// # Arguments
    /// * `name` - Layer name (e.g., "beauty", "diffuse")
    /// * `data` - Pixel data as Float64Array, length must be width * height * 4
    /// * `precision` - Sample precision (F16, F32, or U32)
    /// * `compression` - Compression method (defaults to RLE)
    #[wasm_bindgen(js_name = addRgbaLayer)]
    pub fn add_rgba_layer(
        &mut self,
        name: &str,
        data: &[f64],
        precision: SamplePrecision,
        compression: Option<CompressionMethod>,
    ) -> std::result::Result<(), JsValue> {
        let expected_len = self.width * self.height * 4;
        if data.len() != expected_len {
            return Err(JsValue::from_str(&format!(
                "RGBA layer '{}' expects {} floats ({}x{}x4), got {}",
                name, expected_len, self.width, self.height, data.len()
            )));
        }

        self.layers.push(LayerData {
            name: name.to_string(),
            channel_type: ChannelType::Rgba,
            pixels: data.to_vec(),
            sample_type: precision.into(),
            compression: compression.unwrap_or_default().into(),
        });

        Ok(())
    }

    /// Add an RGB layer (3 channels: R, G, B).
    ///
    /// # Arguments
    /// * `name` - Layer name (e.g., "normals", "albedo")
    /// * `data` - Pixel data as Float64Array, length must be width * height * 3
    /// * `precision` - Sample precision (F16, F32, or U32)
    /// * `compression` - Compression method (defaults to RLE)
    #[wasm_bindgen(js_name = addRgbLayer)]
    pub fn add_rgb_layer(
        &mut self,
        name: &str,
        data: &[f64],
        precision: SamplePrecision,
        compression: Option<CompressionMethod>,
    ) -> std::result::Result<(), JsValue> {
        let expected_len = self.width * self.height * 3;
        if data.len() != expected_len {
            return Err(JsValue::from_str(&format!(
                "RGB layer '{}' expects {} floats ({}x{}x3), got {}",
                name, expected_len, self.width, self.height, data.len()
            )));
        }

        self.layers.push(LayerData {
            name: name.to_string(),
            channel_type: ChannelType::Rgb,
            pixels: data.to_vec(),
            sample_type: precision.into(),
            compression: compression.unwrap_or_default().into(),
        });

        Ok(())
    }

    /// Add a single-channel layer with a custom channel name.
    ///
    /// # Arguments
    /// * `name` - Layer name
    /// * `channel_name` - Channel name (e.g., "Z" for depth, "A" for alpha)
    /// * `data` - Pixel data as Float64Array, length must be width * height
    /// * `precision` - Sample precision (F16, F32, or U32)
    /// * `compression` - Compression method (defaults to RLE)
    #[wasm_bindgen(js_name = addSingleChannelLayer)]
    pub fn add_single_channel_layer(
        &mut self,
        name: &str,
        channel_name: &str,
        data: &[f64],
        precision: SamplePrecision,
        compression: Option<CompressionMethod>,
    ) -> std::result::Result<(), JsValue> {
        let expected_len = self.width * self.height;
        if data.len() != expected_len {
            return Err(JsValue::from_str(&format!(
                "Single-channel layer '{}' expects {} floats ({}x{}), got {}",
                name, expected_len, self.width, self.height, data.len()
            )));
        }

        self.layers.push(LayerData {
            name: name.to_string(),
            channel_type: ChannelType::Single(channel_name.to_string()),
            pixels: data.to_vec(),
            sample_type: precision.into(),
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

        // Build layers (each layer has its own compression)
        let layers: smallvec::SmallVec<[Layer<AnyChannels<FlatSamples>>; 2]> = self
            .layers
            .iter()
            .map(|layer_data| self.build_layer(layer_data, size))
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

    fn build_layer(
        &self,
        layer_data: &LayerData,
        size: Vec2<usize>,
    ) -> Layer<AnyChannels<FlatSamples>> {
        let channels = match &layer_data.channel_type {
            ChannelType::Rgba => self.build_rgba_channels(layer_data, size),
            ChannelType::Rgb => self.build_rgb_channels(layer_data, size),
            ChannelType::Single(channel_name) => {
                self.build_single_channel(layer_data, channel_name, size)
            }
        };

        let encoding = Encoding {
            compression: layer_data.compression,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        };

        Layer::new(
            size,
            LayerAttributes::named(layer_data.name.as_str()),
            encoding,
            channels,
        )
    }

    fn build_rgba_channels(&self, layer_data: &LayerData, size: Vec2<usize>) -> AnyChannels<FlatSamples> {
        let pixel_count = size.0 * size.1;
        let mut r = Vec::with_capacity(pixel_count);
        let mut g = Vec::with_capacity(pixel_count);
        let mut b = Vec::with_capacity(pixel_count);
        let mut a = Vec::with_capacity(pixel_count);

        // Deinterleave RGBA data
        for i in 0..pixel_count {
            r.push(layer_data.pixels[i * 4]);
            g.push(layer_data.pixels[i * 4 + 1]);
            b.push(layer_data.pixels[i * 4 + 2]);
            a.push(layer_data.pixels[i * 4 + 3]);
        }

        let sample_type = layer_data.sample_type;
        AnyChannels::sort(smallvec::smallvec![
            self.make_channel("A", a, sample_type),
            self.make_channel("B", b, sample_type),
            self.make_channel("G", g, sample_type),
            self.make_channel("R", r, sample_type),
        ])
    }

    fn build_rgb_channels(&self, layer_data: &LayerData, size: Vec2<usize>) -> AnyChannels<FlatSamples> {
        let pixel_count = size.0 * size.1;
        let mut r = Vec::with_capacity(pixel_count);
        let mut g = Vec::with_capacity(pixel_count);
        let mut b = Vec::with_capacity(pixel_count);

        // Deinterleave RGB data
        for i in 0..pixel_count {
            r.push(layer_data.pixels[i * 3]);
            g.push(layer_data.pixels[i * 3 + 1]);
            b.push(layer_data.pixels[i * 3 + 2]);
        }

        let sample_type = layer_data.sample_type;
        AnyChannels::sort(smallvec::smallvec![
            self.make_channel("B", b, sample_type),
            self.make_channel("G", g, sample_type),
            self.make_channel("R", r, sample_type),
        ])
    }

    fn build_single_channel(
        &self,
        layer_data: &LayerData,
        channel_name: &str,
        _size: Vec2<usize>,
    ) -> AnyChannels<FlatSamples> {
        let sample_type = layer_data.sample_type;
        AnyChannels::sort(smallvec::smallvec![self.make_channel(
            channel_name,
            layer_data.pixels.clone(),
            sample_type
        )])
    }

    fn make_channel(
        &self,
        name: &str,
        data: Vec<f64>,
        sample_type: exr::meta::attribute::SampleType,
    ) -> AnyChannel<FlatSamples> {
        use exr::meta::attribute::SampleType;

        let samples = match sample_type {
            SampleType::F16 => {
                FlatSamples::F16(data.into_iter().map(|v| half::f16::from_f64(v)).collect())
            }
            SampleType::F32 => FlatSamples::F32(data.into_iter().map(|v| v as f32).collect()),
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
    samples: Vec<f64>,
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
    /// Returns the data as Float64Array (all sample types converted to f64).
    #[wasm_bindgen(js_name = getChannelData)]
    pub fn get_channel_data(&self, layer_index: usize, channel_name: &str) -> Option<Vec<f64>> {
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
    pub fn get_rgba_data(&self, layer_index: usize) -> Option<Vec<f64>> {
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
    pub fn get_rgb_data(&self, layer_index: usize) -> Option<Vec<f64>> {
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
///
/// Returns an ExrDecoder containing all layers and channels.
#[wasm_bindgen(js_name = readExr)]
pub fn read_exr(data: &[u8]) -> std::result::Result<ExrDecoder, JsValue> {
    read_exr_internal(data).map_err(|e| JsValue::from_str(&format!("EXR read error: {}", e)))
}

/// Result of optimized RGBA reading.
#[wasm_bindgen]
pub struct ExrRgbaResult {
    width: u32,
    height: u32,
    data: Vec<f64>,
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

    /// Get the interleaved RGBA pixel data as Float64Array.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Vec<f64> {
        self.data.clone()
    }
}

/// Read an EXR file expecting RGBA channels.
///
/// This is an optimized function that reads RGBA data directly into
/// interleaved format. More efficient than `readExr()` when you know
/// the image has RGBA channels.
///
/// Returns the first valid layer with RGBA channels.
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
            move |resolution, _channels| -> Vec<f64> {
                img_width_create.set(resolution.width());
                vec![0.0f64; resolution.width() * resolution.height() * 4]
            },
            move |pixels, position, (r, g, b, a): (f32, f32, f32, f32)| {
                let width = img_width_set.get();
                let idx = (position.y() * width + position.x()) * 4;
                pixels[idx] = r as f64;
                pixels[idx + 1] = g as f64;
                pixels[idx + 2] = b as f64;
                pixels[idx + 3] = a as f64;
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
    data: Vec<f64>,
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

    /// Get the interleaved RGB pixel data as Float64Array.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Vec<f64> {
        self.data.clone()
    }
}

/// Read an EXR file expecting RGB channels.
///
/// This is an optimized function that reads RGB data directly into
/// interleaved format. More efficient than `readExr()` when you know
/// the image has RGB channels.
///
/// Returns the first valid layer with RGB channels.
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
            move |resolution, _channels| -> Vec<f64> {
                img_width_create.set(resolution.width());
                vec![0.0f64; resolution.width() * resolution.height() * 3]
            },
            move |pixels, position, (r, g, b): (f32, f32, f32)| {
                let width = img_width_set.get();
                let idx = (position.y() * width + position.x()) * 3;
                pixels[idx] = r as f64;
                pixels[idx + 1] = g as f64;
                pixels[idx + 2] = b as f64;
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
/// This is a convenience function for simple single-layer images.
/// No `.free()` call is needed - the result is returned directly.
///
/// # Arguments
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `layer_name` - Layer name (e.g., "beauty")
/// * `data` - RGBA pixel data as Float64Array, length must be width * height * 4
/// * `precision` - Sample precision (F16, F32, or U32)
/// * `compression` - Compression method
#[wasm_bindgen(js_name = writeExrRgba)]
pub fn write_exr_rgba(
    width: u32,
    height: u32,
    layer_name: &str,
    data: &[f64],
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
/// * `data` - RGB pixel data as Float64Array, length must be width * height * 3
/// * `precision` - Sample precision (F16, F32, or U32)
/// * `compression` - Compression method
#[wasm_bindgen(js_name = writeExrRgb)]
pub fn write_exr_rgb(
    width: u32,
    height: u32,
    layer_name: &str,
    data: &[f64],
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
/// * `data` - Pixel data as Float64Array, length must be width * height
/// * `precision` - Sample precision (F16, F32, or U32)
/// * `compression` - Compression method
#[wasm_bindgen(js_name = writeExrSingleChannel)]
pub fn write_exr_single_channel(
    width: u32,
    height: u32,
    layer_name: &str,
    channel_name: &str,
    data: &[f64],
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
                    samples: channel.sample_data.values_as_f32().map(|v| v as f64).collect(),
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

/// Native tests (not using wasm-bindgen types)
#[cfg(test)]
mod tests {
    use super::*;

    /// Test that we can create and encode a simple RGBA image
    #[test]
    fn test_create_simple_rgba() {
        let mut image = ExrEncoder {
            width: 4,
            height: 4,
            layers: Vec::new(),
        };

        let pixels = vec![0.5f64; 4 * 4 * 4];
        image.layers.push(LayerData {
            name: "test".to_string(),
            channel_type: ChannelType::Rgba,
            pixels,
            sample_type: exr::meta::attribute::SampleType::F32,
            compression: Compression::Uncompressed,
        });

        let bytes = image.build_and_encode().unwrap();
        assert!(!bytes.is_empty());
        // Check EXR magic number
        assert_eq!(&bytes[0..4], &[0x76, 0x2f, 0x31, 0x01]);
    }

    /// Test multi-layer EXR creation
    #[test]
    fn test_multi_layer() {
        let mut image = ExrEncoder {
            width: 8,
            height: 8,
            layers: Vec::new(),
        };

        // Add beauty layer (RGBA)
        image.layers.push(LayerData {
            name: "beauty".to_string(),
            channel_type: ChannelType::Rgba,
            pixels: vec![1.0f64; 8 * 8 * 4],
            sample_type: exr::meta::attribute::SampleType::F32,
            compression: Compression::Uncompressed,
        });

        // Add normals layer (RGB)
        image.layers.push(LayerData {
            name: "normals".to_string(),
            channel_type: ChannelType::Rgb,
            pixels: vec![0.5f64; 8 * 8 * 3],
            sample_type: exr::meta::attribute::SampleType::F32,
            compression: Compression::Uncompressed,
        });

        // Add depth layer (single channel)
        image.layers.push(LayerData {
            name: "depth".to_string(),
            channel_type: ChannelType::Single("Z".to_string()),
            pixels: vec![0.0f64; 8 * 8],
            sample_type: exr::meta::attribute::SampleType::F32,
            compression: Compression::Uncompressed,
        });

        assert_eq!(image.layers.len(), 3);

        let bytes = image.build_and_encode().unwrap();
        assert!(!bytes.is_empty());
    }

    /// Test compression methods
    #[test]
    fn test_compression_methods() {
        for compression in [
            Compression::Uncompressed,
            Compression::RLE,
            Compression::ZIP1,
            Compression::ZIP16,
            Compression::PIZ,
        ] {
            let mut image = ExrEncoder {
                width: 16,
                height: 16,
                layers: Vec::new(),
            };

            image.layers.push(LayerData {
                name: "test".to_string(),
                channel_type: ChannelType::Rgba,
                pixels: vec![0.5f64; 16 * 16 * 4],
                sample_type: exr::meta::attribute::SampleType::F32,
                compression,
            });

            let bytes = image.build_and_encode().unwrap();
            assert!(!bytes.is_empty(), "Compression {:?} failed", compression);
        }
    }

    /// Test F16 sample type
    #[test]
    fn test_f16_samples() {
        let mut image = ExrEncoder {
            width: 4,
            height: 4,
            layers: Vec::new(),
        };

        image.layers.push(LayerData {
            name: "test_f16".to_string(),
            channel_type: ChannelType::Rgba,
            pixels: vec![0.5f64; 4 * 4 * 4],
            sample_type: exr::meta::attribute::SampleType::F16,
            compression: Compression::Uncompressed,
        });

        let bytes = image.build_and_encode().unwrap();
        assert!(!bytes.is_empty());
    }

    /// Test roundtrip: write RGBA then read back
    #[test]
    fn test_roundtrip_rgba() {
        let width = 4;
        let height = 4;
        let pixel_count = width * height;

        // Create test data with distinct values
        let mut pixels = Vec::with_capacity(pixel_count * 4);
        for i in 0..pixel_count {
            pixels.push(i as f64 / pixel_count as f64); // R
            pixels.push(0.5); // G
            pixels.push(0.25); // B
            pixels.push(1.0); // A
        }

        let mut image = ExrEncoder {
            width,
            height,
            layers: Vec::new(),
        };

        image.layers.push(LayerData {
            name: "test".to_string(),
            channel_type: ChannelType::Rgba,
            pixels: pixels.clone(),
            sample_type: exr::meta::attribute::SampleType::F32,
            compression: Compression::Uncompressed,
        });

        // Write
        let bytes = image.build_and_encode().unwrap();

        // Read back
        let read_result = read_exr_internal(&bytes).unwrap();

        // Verify dimensions
        assert_eq!(read_result.width, width as u32);
        assert_eq!(read_result.height, height as u32);

        // Verify layer count
        assert_eq!(read_result.layers.len(), 1);

        // Verify we can get RGBA data back
        let rgba_data = read_result.get_rgba_data(0).expect("Should have RGBA data");
        assert_eq!(rgba_data.len(), pixel_count * 4);

        // Verify values match (with small epsilon for floating point)
        for (i, (original, read)) in pixels.iter().zip(rgba_data.iter()).enumerate() {
            assert!(
                (original - read).abs() < 0.001,
                "Mismatch at index {}: {} vs {}",
                i,
                original,
                read
            );
        }
    }

    /// Test roundtrip: write RGB then read back
    #[test]
    fn test_roundtrip_rgb() {
        let width = 4;
        let height = 4;
        let pixel_count = width * height;

        let pixels: Vec<f64> = (0..pixel_count * 3).map(|i| i as f64 / 100.0).collect();

        let mut image = ExrEncoder {
            width,
            height,
            layers: Vec::new(),
        };

        image.layers.push(LayerData {
            name: "normals".to_string(),
            channel_type: ChannelType::Rgb,
            pixels: pixels.clone(),
            sample_type: exr::meta::attribute::SampleType::F32,
            compression: Compression::Uncompressed,
        });

        let bytes = image.build_and_encode().unwrap();
        let read_result = read_exr_internal(&bytes).unwrap();

        let rgb_data = read_result.get_rgb_data(0).expect("Should have RGB data");
        assert_eq!(rgb_data.len(), pixel_count * 3);

        for (original, read) in pixels.iter().zip(rgb_data.iter()) {
            assert!((original - read).abs() < 0.001);
        }
    }

    /// Test roundtrip: write single channel then read back
    #[test]
    fn test_roundtrip_single_channel() {
        let width = 4;
        let height = 4;
        let pixel_count = width * height;

        let pixels: Vec<f64> = (0..pixel_count).map(|i| i as f64).collect();

        let mut image = ExrEncoder {
            width,
            height,
            layers: Vec::new(),
        };

        image.layers.push(LayerData {
            name: "depth".to_string(),
            channel_type: ChannelType::Single("Z".to_string()),
            pixels: pixels.clone(),
            sample_type: exr::meta::attribute::SampleType::F32,
            compression: Compression::Uncompressed,
        });

        let bytes = image.build_and_encode().unwrap();
        let read_result = read_exr_internal(&bytes).unwrap();

        let z_data = read_result
            .get_channel_data(0, "Z")
            .expect("Should have Z channel");
        assert_eq!(z_data.len(), pixel_count);

        for (original, read) in pixels.iter().zip(z_data.iter()) {
            assert!((original - read).abs() < 0.001);
        }
    }

    /// Test roundtrip with multiple layers
    #[test]
    fn test_roundtrip_multi_layer() {
        let width = 4;
        let height = 4;

        let rgba_pixels = vec![0.8f64; width * height * 4];
        let rgb_pixels = vec![0.5f64; width * height * 3];
        let depth_pixels = vec![1.0f64; width * height];

        let mut image = ExrEncoder {
            width,
            height,
            layers: Vec::new(),
        };

        image.layers.push(LayerData {
            name: "beauty".to_string(),
            channel_type: ChannelType::Rgba,
            pixels: rgba_pixels.clone(),
            sample_type: exr::meta::attribute::SampleType::F32,
            compression: Compression::Uncompressed,
        });

        image.layers.push(LayerData {
            name: "normals".to_string(),
            channel_type: ChannelType::Rgb,
            pixels: rgb_pixels.clone(),
            sample_type: exr::meta::attribute::SampleType::F32,
            compression: Compression::Uncompressed,
        });

        image.layers.push(LayerData {
            name: "depth".to_string(),
            channel_type: ChannelType::Single("Z".to_string()),
            pixels: depth_pixels.clone(),
            sample_type: exr::meta::attribute::SampleType::F32,
            compression: Compression::Uncompressed,
        });

        let bytes = image.build_and_encode().unwrap();
        let read_result = read_exr_internal(&bytes).unwrap();

        assert_eq!(read_result.layers.len(), 3);

        // Verify beauty layer
        let beauty_rgba = read_result.get_rgba_data(0).expect("Should have beauty RGBA");
        assert_eq!(beauty_rgba.len(), width * height * 4);

        // Verify normals layer
        let normals_rgb = read_result.get_rgb_data(1).expect("Should have normals RGB");
        assert_eq!(normals_rgb.len(), width * height * 3);

        // Verify depth layer
        let depth_z = read_result
            .get_channel_data(2, "Z")
            .expect("Should have depth Z");
        assert_eq!(depth_z.len(), width * height);
    }
}
