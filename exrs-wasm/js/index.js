/**
 * exrs-wasm - JavaScript wrapper for writing and reading EXR files in the browser.
 *
 * This module provides a clean, easy-to-use API for EXR files.
 * Call init() once before using other functions.
 */

// WASM module reference (set by init())
let wasmModule = null;

/**
 * Initialize the WASM module. Must be called before using other functions.
 * @returns {Promise<void>}
 *
 * @example
 * import { init, encodeExr, decodeExr } from 'exrs-wasm';
 * await init();
 * // Now use encodeExr/decodeExr synchronously
 */
export async function init() {
  if (wasmModule) return;

  const wasm = await import('../pkg/exrs_wasm.js');
  await wasm.default();
  wasmModule = wasm;
}

function ensureInitialized() {
  if (!wasmModule) {
    throw new Error('WASM module not initialized. Call init() first.');
  }
}

/**
 * Encode pixel data into an EXR file.
 *
 * @param {Object} options - Encoding options
 * @param {number} options.width - Image width in pixels
 * @param {number} options.height - Image height in pixels
 * @param {Array<Object>} options.layers - Array of layer definitions
 * @param {string} options.layers[].name - Layer name (e.g., "beauty", "depth")
 * @param {'rgba'|'rgb'|string[]} options.layers[].channels - Channel type: 'rgba', 'rgb', or array of channel names like ['Z']
 * @param {Float64Array} options.layers[].data - Pixel data as Float64Array
 * @param {'f16'|'f32'|'u32'} [options.layers[].precision='f32'] - Sample precision
 * @param {'none'|'rle'|'zip'|'zip16'|'piz'|'pxr24'} [options.layers[].compression='rle'] - Compression method
 * @returns {Uint8Array} EXR file bytes
 *
 * @example
 * await init();
 * const bytes = encodeExr({
 *   width: 1920,
 *   height: 1080,
 *   layers: [
 *     { name: 'beauty', channels: 'rgba', data: rgbaData, precision: 'f32', compression: 'piz' },
 *     { name: 'depth', channels: ['Z'], data: depthData, precision: 'f32', compression: 'pxr24' }
 *   ]
 * });
 */
export function encodeExr(options) {
  ensureInitialized();

  const { width, height, layers } = options;

  // Map string precision/compression to WASM enum values
  const precisionMap = {
    f16: wasmModule.SamplePrecision.F16,
    f32: wasmModule.SamplePrecision.F32,
    u32: wasmModule.SamplePrecision.U32,
  };

  const compressionMap = {
    none: wasmModule.CompressionMethod.None,
    rle: wasmModule.CompressionMethod.Rle,
    zip: wasmModule.CompressionMethod.Zip,
    zip16: wasmModule.CompressionMethod.Zip16,
    piz: wasmModule.CompressionMethod.Piz,
    pxr24: wasmModule.CompressionMethod.Pxr24,
  };

  // Single layer shortcuts
  if (layers.length === 1) {
    const layer = layers[0];
    const precision = precisionMap[layer.precision || 'f32'];
    const compression = compressionMap[layer.compression || 'rle'];

    if (layer.channels === 'rgba') {
      return wasmModule.writeExrRgba(width, height, layer.name, layer.data, precision, compression);
    } else if (layer.channels === 'rgb') {
      return wasmModule.writeExrRgb(width, height, layer.name, layer.data, precision, compression);
    } else if (Array.isArray(layer.channels) && layer.channels.length === 1) {
      return wasmModule.writeExrSingleChannel(
        width, height, layer.name, layer.channels[0], layer.data, precision, compression
      );
    }
  }

  // Multi-layer: use the encoder
  const encoder = new wasmModule.ExrEncoder(width, height);

  try {
    for (const layer of layers) {
      const precision = precisionMap[layer.precision || 'f32'];
      const compression = compressionMap[layer.compression || 'rle'];

      if (layer.channels === 'rgba') {
        encoder.addRgbaLayer(layer.name, layer.data, precision, compression);
      } else if (layer.channels === 'rgb') {
        encoder.addRgbLayer(layer.name, layer.data, precision, compression);
      } else if (Array.isArray(layer.channels) && layer.channels.length === 1) {
        encoder.addSingleChannelLayer(layer.name, layer.channels[0], layer.data, precision, compression);
      } else {
        throw new Error(`Unsupported channels format: ${JSON.stringify(layer.channels)}`);
      }
    }

    return encoder.toBytes();
  } finally {
    // Free is optional with FinalizationRegistry, but we call it for deterministic cleanup
    encoder.free();
  }
}

/**
 * Decode an EXR file into pixel data.
 *
 * @param {Uint8Array} data - EXR file bytes
 * @returns {Object} Decoded image data
 * @returns {number} return.width - Image width in pixels
 * @returns {number} return.height - Image height in pixels
 * @returns {Array<Object>} return.layers - Array of decoded layers
 * @returns {string|null} return.layers[].name - Layer name (null for default layer)
 * @returns {string[]} return.layers[].channels - Channel names in this layer
 * @returns {Function} return.layers[].getData - Get interleaved data (auto-detects RGBA/RGB/single channel)
 * @returns {Function} return.layers[].getChannel - Get data for a specific channel by name
 *
 * @example
 * await init();
 * const image = decodeExr(bytes);
 * console.log(image.width, image.height);
 *
 * // Get pixel data (auto-detects format based on channels)
 * const pixelData = image.layers[0].getData();
 *
 * // Get a specific channel by name
 * const depthData = image.layers[1].getChannel('Z');
 */
export function decodeExr(data) {
  ensureInitialized();

  const decoder = wasmModule.readExr(data);

  try {
    const width = decoder.width;
    const height = decoder.height;
    const layerCount = decoder.layerCount;

    const layers = [];
    for (let i = 0; i < layerCount; i++) {
      const name = decoder.getLayerName(i);
      const channels = decoder.getChannelNames(i);

      // Determine channel type from actual channel names
      const hasR = channels.includes('R');
      const hasG = channels.includes('G');
      const hasB = channels.includes('B');
      const hasA = channels.includes('A');
      const isRgba = hasR && hasG && hasB && hasA;
      const isRgb = hasR && hasG && hasB && !hasA;

      layers.push({
        name,
        channels,
        // Auto-detect format based on channel names
        getData: () => {
          if (isRgba) {
            return decoder.getRgbaData(i);
          } else if (isRgb) {
            return decoder.getRgbData(i);
          } else if (channels.length === 1) {
            return decoder.getChannelData(i, channels[0]);
          } else {
            // Multiple non-RGB channels - return first channel
            return decoder.getChannelData(i, channels[0]);
          }
        },
        // Get specific channel by name
        getChannel: (channelName) => decoder.getChannelData(i, channelName),
      });
    }

    return { width, height, layers };
  } finally {
    decoder.free();
  }
}

/**
 * Decode an EXR file expecting RGBA channels (optimized path).
 *
 * This is faster than decodeExr() when you know the image has RGBA channels.
 *
 * @param {Uint8Array} data - EXR file bytes
 * @returns {Object} Decoded image data
 * @returns {number} return.width - Image width in pixels
 * @returns {number} return.height - Image height in pixels
 * @returns {Float64Array} return.data - Interleaved RGBA pixel data
 *
 * @example
 * await init();
 * const { width, height, data } = decodeExrRgba(bytes);
 */
export function decodeExrRgba(data) {
  ensureInitialized();

  const result = wasmModule.readExrRgba(data);
  try {
    return {
      width: result.width,
      height: result.height,
      data: result.data,
    };
  } finally {
    result.free();
  }
}

/**
 * Decode an EXR file expecting RGB channels (optimized path).
 *
 * This is faster than decodeExr() when you know the image has RGB channels.
 *
 * @param {Uint8Array} data - EXR file bytes
 * @returns {Object} Decoded image data
 * @returns {number} return.width - Image width in pixels
 * @returns {number} return.height - Image height in pixels
 * @returns {Float64Array} return.data - Interleaved RGB pixel data
 *
 * @example
 * await init();
 * const { width, height, data } = decodeExrRgb(bytes);
 */
export function decodeExrRgb(data) {
  ensureInitialized();

  const result = wasmModule.readExrRgb(data);
  try {
    return {
      width: result.width,
      height: result.height,
      data: result.data,
    };
  } finally {
    result.free();
  }
}
