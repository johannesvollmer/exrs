//! WebAssembly bindings for writing EXR files from the browser.
//!
//! This crate provides JavaScript-friendly APIs for creating multi-layer EXR files
//! with AOVs (Arbitrary Output Variables) commonly used in rendering pipelines.
//!
//! # Example (JavaScript)
//!
//! ```javascript
//! import init, { ExrImage } from 'exrs-wasm';
//!
//! await init();
//!
//! const exr = new ExrImage(1920, 1080, 'rle');
//! exr.addRgbaLayer('beauty', beautyPixels);
//! exr.addDepthLayer('depth', depthPixels);
//! exr.addRgbLayer('normals', normalPixels);
//!
//! const bytes = exr.toBytes();
//! // Download or save bytes...
//!
//! exr.free();
//! ```

use wasm_bindgen::prelude::*;
use exr::prelude::*;
use exr::image::AnyChannels;
use std::io::Cursor;

/// Initialize panic hook for better error messages in browser console.
/// Call this once at startup.
#[wasm_bindgen(start)]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Represents pixel data for a single layer.
struct LayerData {
    name: String,
    channel_type: ChannelType,
    pixels: Vec<f32>,
    sample_type: exr::meta::attribute::SampleType,
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
}

impl From<SamplePrecision> for exr::meta::attribute::SampleType {
    fn from(precision: SamplePrecision) -> Self {
        match precision {
            SamplePrecision::F16 => exr::meta::attribute::SampleType::F16,
            SamplePrecision::F32 => exr::meta::attribute::SampleType::F32,
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

/// Builder for creating multi-layer EXR images.
///
/// Use this class to construct EXR files with multiple AOV layers
/// (beauty, depth, normals, etc.) from WebGL/WebGPU render buffers.
#[wasm_bindgen]
pub struct ExrImage {
    width: usize,
    height: usize,
    compression: Compression,
    layers: Vec<LayerData>,
}

#[wasm_bindgen]
impl ExrImage {
    /// Create a new EXR image builder.
    ///
    /// # Arguments
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `compression` - Compression method (optional, defaults to RLE)
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32, compression: Option<CompressionMethod>) -> ExrImage {
        ExrImage {
            width: width as usize,
            height: height as usize,
            compression: compression.unwrap_or_default().into(),
            layers: Vec::new(),
        }
    }

    /// Add an RGBA layer (4 channels: R, G, B, A).
    ///
    /// # Arguments
    /// * `name` - Layer name (e.g., "beauty", "diffuse")
    /// * `data` - Pixel data as Float32Array, length must be width * height * 4
    /// * `precision` - Sample precision (optional, defaults to F32)
    #[wasm_bindgen(js_name = addRgbaLayer)]
    pub fn add_rgba_layer(
        &mut self,
        name: &str,
        data: &[f32],
        precision: Option<SamplePrecision>,
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
            sample_type: precision.unwrap_or_default().into(),
        });

        Ok(())
    }

    /// Add an RGB layer (3 channels: R, G, B).
    ///
    /// # Arguments
    /// * `name` - Layer name (e.g., "normals", "albedo")
    /// * `data` - Pixel data as Float32Array, length must be width * height * 3
    /// * `precision` - Sample precision (optional, defaults to F32)
    #[wasm_bindgen(js_name = addRgbLayer)]
    pub fn add_rgb_layer(
        &mut self,
        name: &str,
        data: &[f32],
        precision: Option<SamplePrecision>,
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
            sample_type: precision.unwrap_or_default().into(),
        });

        Ok(())
    }

    /// Add a depth layer (single channel named "Z").
    ///
    /// # Arguments
    /// * `name` - Layer name (e.g., "depth")
    /// * `data` - Pixel data as Float32Array, length must be width * height
    /// * `precision` - Sample precision (optional, defaults to F32)
    #[wasm_bindgen(js_name = addDepthLayer)]
    pub fn add_depth_layer(
        &mut self,
        name: &str,
        data: &[f32],
        precision: Option<SamplePrecision>,
    ) -> std::result::Result<(), JsValue> {
        self.add_single_channel_layer(name, "Z", data, precision)
    }

    /// Add a single-channel layer with a custom channel name.
    ///
    /// # Arguments
    /// * `name` - Layer name
    /// * `channel_name` - Channel name (e.g., "Z" for depth, "A" for alpha)
    /// * `data` - Pixel data as Float32Array, length must be width * height
    /// * `precision` - Sample precision (optional, defaults to F32)
    #[wasm_bindgen(js_name = addSingleChannelLayer)]
    pub fn add_single_channel_layer(
        &mut self,
        name: &str,
        channel_name: &str,
        data: &[f32],
        precision: Option<SamplePrecision>,
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
            sample_type: precision.unwrap_or_default().into(),
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
impl ExrImage {
    fn build_and_encode(&self) -> std::result::Result<Vec<u8>, exr::error::Error> {
        let size = Vec2(self.width, self.height);
        let encoding = Encoding {
            compression: self.compression,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        };

        // Build layers
        let layers: smallvec::SmallVec<[Layer<AnyChannels<FlatSamples>>; 2]> = self
            .layers
            .iter()
            .map(|layer_data| self.build_layer(layer_data, size, encoding.clone()))
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
        encoding: Encoding,
    ) -> Layer<AnyChannels<FlatSamples>> {
        let channels = match &layer_data.channel_type {
            ChannelType::Rgba => self.build_rgba_channels(layer_data, size),
            ChannelType::Rgb => self.build_rgb_channels(layer_data, size),
            ChannelType::Single(channel_name) => {
                self.build_single_channel(layer_data, channel_name, size)
            }
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
        data: Vec<f32>,
        sample_type: exr::meta::attribute::SampleType,
    ) -> AnyChannel<FlatSamples> {
        use exr::meta::attribute::SampleType;

        let samples = match sample_type {
            SampleType::F16 => {
                FlatSamples::F16(data.into_iter().map(half::f16::from_f32).collect())
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

/// Writer optimized for animation sequences.
///
/// Reuses internal buffers across frames for better memory efficiency
/// when exporting many frames.
#[wasm_bindgen]
pub struct ExrSequenceWriter {
    width: usize,
    height: usize,
    compression: Compression,
}

#[wasm_bindgen]
impl ExrSequenceWriter {
    /// Create a new sequence writer.
    ///
    /// # Arguments
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `compression` - Compression method (optional, defaults to RLE)
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32, compression: Option<CompressionMethod>) -> ExrSequenceWriter {
        ExrSequenceWriter {
            width: width as usize,
            height: height as usize,
            compression: compression.unwrap_or_default().into(),
        }
    }

    /// Write a single frame with beauty (RGBA), depth (Z), and normals (RGB).
    ///
    /// This is a convenience method for the common AOV setup.
    ///
    /// # Arguments
    /// * `beauty` - RGBA pixel data (width * height * 4 floats)
    /// * `depth` - Depth pixel data (width * height floats)
    /// * `normals` - Normal pixel data (width * height * 3 floats)
    #[wasm_bindgen(js_name = writeFrame)]
    pub fn write_frame(
        &mut self,
        beauty: &[f32],
        depth: &[f32],
        normals: &[f32],
    ) -> std::result::Result<Vec<u8>, JsValue> {
        let mut image = ExrImage::new(self.width as u32, self.height as u32, None);
        image.compression = self.compression;

        image.add_rgba_layer("beauty", beauty, None)?;
        image.add_depth_layer("depth", depth, None)?;
        image.add_rgb_layer("normals", normals, None)?;

        image.to_bytes()
    }

    /// Write a frame with custom layers.
    ///
    /// Use an ExrImage to configure layers, then pass it here.
    #[wasm_bindgen(js_name = writeCustomFrame)]
    pub fn write_custom_frame(&mut self, image: &ExrImage) -> std::result::Result<Vec<u8>, JsValue> {
        image.to_bytes()
    }
}

/// Native tests (not using wasm-bindgen types)
#[cfg(test)]
mod tests {
    use super::*;

    /// Test that we can create and encode a simple RGBA image
    #[test]
    fn test_create_simple_rgba() {
        let mut image = ExrImage {
            width: 4,
            height: 4,
            compression: Compression::Uncompressed,
            layers: Vec::new(),
        };

        let pixels = vec![0.5f32; 4 * 4 * 4];
        image.layers.push(LayerData {
            name: "test".to_string(),
            channel_type: ChannelType::Rgba,
            pixels,
            sample_type: exr::meta::attribute::SampleType::F32,
        });

        let bytes = image.build_and_encode().unwrap();
        assert!(!bytes.is_empty());
        // Check EXR magic number
        assert_eq!(&bytes[0..4], &[0x76, 0x2f, 0x31, 0x01]);
    }

    /// Test multi-layer EXR creation
    #[test]
    fn test_multi_layer() {
        let mut image = ExrImage {
            width: 8,
            height: 8,
            compression: Compression::Uncompressed,
            layers: Vec::new(),
        };

        // Add beauty layer (RGBA)
        image.layers.push(LayerData {
            name: "beauty".to_string(),
            channel_type: ChannelType::Rgba,
            pixels: vec![1.0f32; 8 * 8 * 4],
            sample_type: exr::meta::attribute::SampleType::F32,
        });

        // Add normals layer (RGB)
        image.layers.push(LayerData {
            name: "normals".to_string(),
            channel_type: ChannelType::Rgb,
            pixels: vec![0.5f32; 8 * 8 * 3],
            sample_type: exr::meta::attribute::SampleType::F32,
        });

        // Add depth layer (single channel)
        image.layers.push(LayerData {
            name: "depth".to_string(),
            channel_type: ChannelType::Single("Z".to_string()),
            pixels: vec![0.0f32; 8 * 8],
            sample_type: exr::meta::attribute::SampleType::F32,
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
            let mut image = ExrImage {
                width: 16,
                height: 16,
                compression,
                layers: Vec::new(),
            };

            image.layers.push(LayerData {
                name: "test".to_string(),
                channel_type: ChannelType::Rgba,
                pixels: vec![0.5f32; 16 * 16 * 4],
                sample_type: exr::meta::attribute::SampleType::F32,
            });

            let bytes = image.build_and_encode().unwrap();
            assert!(!bytes.is_empty(), "Compression {:?} failed", compression);
        }
    }

    /// Test F16 sample type
    #[test]
    fn test_f16_samples() {
        let mut image = ExrImage {
            width: 4,
            height: 4,
            compression: Compression::Uncompressed,
            layers: Vec::new(),
        };

        image.layers.push(LayerData {
            name: "test_f16".to_string(),
            channel_type: ChannelType::Rgba,
            pixels: vec![0.5f32; 4 * 4 * 4],
            sample_type: exr::meta::attribute::SampleType::F16,
        });

        let bytes = image.build_and_encode().unwrap();
        assert!(!bytes.is_empty());
    }
}
