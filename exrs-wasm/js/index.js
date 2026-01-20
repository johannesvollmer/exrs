/**
 * exrs-wasm - JavaScript wrapper for writing and reading EXR files in the browser.
 *
 * This module provides a clean, easy-to-use API that handles WASM initialization
 * automatically.
 */

// Lazy-loaded WASM module
let wasmModule = null;
let initPromise = null;

/**
 * Initialize the WASM module (called automatically on first use).
 * @returns {Promise<void>}
 */
async function ensureInitialized() {
  if (wasmModule) return;

  if (!initPromise) {
    // Dynamic import of the wasm-bindgen generated module
    initPromise = (async () => {
      const wasm = await import('../pkg/exrs_wasm.js');
      await wasm.default();
      wasmModule = wasm;
    })();
  }

  await initPromise;
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
 * @returns {Promise<Uint8Array>} EXR file bytes
 *
 * @example
 * const bytes = await encodeExr({
 *   width: 1920,
 *   height: 1080,
 *   layers: [
 *     { name: 'beauty', channels: 'rgba', data: rgbaData, precision: 'f32', compression: 'piz' },
 *     { name: 'depth', channels: ['Z'], data: depthData, precision: 'f32', compression: 'pxr24' }
 *   ]
 * });
 */
export async function encodeExr(options) {
  await ensureInitialized();

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
 * @returns {Promise<Object>} Decoded image data
 * @returns {number} return.width - Image width in pixels
 * @returns {number} return.height - Image height in pixels
 * @returns {Array<Object>} return.layers - Array of decoded layers
 * @returns {string|null} return.layers[].name - Layer name (null for default layer)
 * @returns {string[]} return.layers[].channels - Channel names in this layer
 * @returns {Function} return.layers[].getData - Function to get interleaved data: getData('rgba'|'rgb'|channelName)
 *
 * @example
 * const image = await decodeExr(bytes);
 * console.log(image.width, image.height);
 *
 * // Get RGBA data for first layer
 * const rgbaData = image.layers[0].getData('rgba');
 *
 * // Get depth channel from second layer
 * const depthData = image.layers[1].getData('Z');
 */
export async function decodeExr(data) {
  await ensureInitialized();

  const decoder = wasmModule.readExr(data);

  try {
    const width = decoder.width;
    const height = decoder.height;
    const layerCount = decoder.layerCount;

    const layers = [];
    for (let i = 0; i < layerCount; i++) {
      const name = decoder.getLayerName(i);
      const channels = decoder.getChannelNames(i);

      layers.push({
        name,
        channels,
        getData: (type) => {
          if (type === 'rgba') {
            return decoder.getRgbaData(i);
          } else if (type === 'rgb') {
            return decoder.getRgbData(i);
          } else {
            return decoder.getChannelData(i, type);
          }
        },
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
 * @returns {Promise<Object>} Decoded image data
 * @returns {number} return.width - Image width in pixels
 * @returns {number} return.height - Image height in pixels
 * @returns {Float64Array} return.data - Interleaved RGBA pixel data
 *
 * @example
 * const { width, height, data } = await decodeExrRgba(bytes);
 */
export async function decodeExrRgba(data) {
  await ensureInitialized();

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
 * @returns {Promise<Object>} Decoded image data
 * @returns {number} return.width - Image width in pixels
 * @returns {number} return.height - Image height in pixels
 * @returns {Float64Array} return.data - Interleaved RGB pixel data
 *
 * @example
 * const { width, height, data } = await decodeExrRgb(bytes);
 */
export async function decodeExrRgb(data) {
  await ensureInitialized();

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

// Re-export enums for advanced usage
export { ensureInitialized as init };
