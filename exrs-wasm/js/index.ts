/**
 * exrs-wasm - JavaScript wrapper for writing and reading EXR files in the browser.
 *
 * This module provides a clean, easy-to-use API for EXR files.
 * Call init() once before using other functions.
 */


/** Sample precision for pixel data */
export type Precision = 'f16' | 'f32' | 'u32';

/** Compression method for EXR output */
export type Compression = 'none' | 'rle' | 'zip' | 'zip16' | 'piz' | 'pxr24';

/** Channel type specification */
export type Channels = 'rgba' | 'rgb' | string[];

/** Layer definition for encoding */
export interface ExrLayer {
  /** Layer name (e.g., "beauty", "depth") */
  name: string;
  /** Channel type: 'rgba', 'rgb', or array of channel names like ['Z'] */
  channels: Channels;
  /** Pixel data as Float64Array */
  data: Float64Array;
  /** Sample precision (default: 'f32') */
  precision?: Precision;
  /** Compression method (default: 'rle') */
  compression?: Compression;
}

/** Options for encoding an EXR file */
export interface ExrEncodeOptions {
  /** Image width in pixels */
  width: number;
  /** Image height in pixels */
  height: number;
  /** Array of layer definitions */
  layers: ExrLayer[];
}

/** Decoded layer information */
export interface DecodedLayer {
  /** Layer name (null for default layer) */
  name: string | null;
  /** Channel names in this layer */
  channels: string[];
  /**
   * Get interleaved pixel data for this layer.
   * Auto-detects format based on channel names (RGBA, RGB, or single channel).
   * @returns Pixel data as Float64Array, or null if channels don't exist
   */
  getData(): Float64Array | null;
  /**
   * Get data for a specific channel by name.
   * @param channelName - Channel name like 'R', 'G', 'B', 'A', 'Z', etc.
   * @returns Pixel data as Float64Array, or null if channel doesn't exist
   */
  getChannel(channelName: string): Float64Array | null;
}

/** Result of decoding an EXR file */
export interface ExrDecodeResult {
  /** Image width in pixels */
  width: number;
  /** Image height in pixels */
  height: number;
  /** Array of decoded layers */
  layers: DecodedLayer[];
}

/** Result of optimized RGBA decoding */
export interface ExrRgbaDecodeResult {
  /** Image width in pixels */
  width: number;
  /** Image height in pixels */
  height: number;
  /** Interleaved RGBA pixel data */
  data: Float64Array;
}

/** Result of optimized RGB decoding */
export interface ExrRgbDecodeResult {
  /** Image width in pixels */
  width: number;
  /** Image height in pixels */
  height: number;
  /** Interleaved RGB pixel data */
  data: Float64Array;
}

// WASM module reference (set by init())
let wasmModule: any = null;

/**
 * Initialize the WASM module. Must be called before using other functions.
 * @returns {Promise<void>}
 *
 * @example
 * import { init, encodeExr, decodeExr } from 'exrs-wasm';
 * await init();
 * // Now use encodeExr/decodeExr synchronously
 */
export async function init(): Promise<void> {
  if (wasmModule) return;

  let wasm;
  try {
    // Try browser-style loading
    wasm = await import('exrs-raw-wasm-bindgen');
    await wasm.default();
  } catch (e) {
    // Fallback for Node.js/Vitest
    try {
      const fs = await import('fs');
      const path = await import('path');
      const { createRequire } = await import('module');
      const require = createRequire(import.meta.url);
      
      // Resolve the package path
      const packagePath = path.dirname(require.resolve('exrs-raw-wasm-bindgen/package.json'));
      const wasmPath = path.resolve(packagePath, 'exrs_raw_wasm_bindgen_bg.wasm');
      const wasmBuffer = fs.readFileSync(wasmPath);
      
      wasm = await import('exrs-raw-wasm-bindgen');
      await wasm.initSync({ module: wasmBuffer });
    } catch (nodeErr) {
      console.error('Failed to initialize WASM in both browser and Node environments');
      throw e;
    }
  }
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
 * @param {ExrEncodeOptions} options - Encoding options
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
export function encodeExr(options: ExrEncodeOptions): Uint8Array {
  ensureInitialized();

  const { width, height, layers } = options;

  // Map string precision/compression to WASM enum values
  const precisionMap: Record<Precision, any> = {
    f16: wasmModule.SamplePrecision.F16,
    f32: wasmModule.SamplePrecision.F32,
    u32: wasmModule.SamplePrecision.U32,
  };

  const compressionMap: Record<Compression, any> = {
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
 * @returns {ExrDecodeResult} Decoded image data
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
export function decodeExr(data: Uint8Array): ExrDecodeResult {
  ensureInitialized();

  const decoder = wasmModule.readExr(data);

  const width = decoder.width;
  const height = decoder.height;
  const layerCount = decoder.layerCount;

  const layers: DecodedLayer[] = [];
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
      getChannel: (channelName: string) => decoder.getChannelData(i, channelName),
    });
  }

  // Note: We are NOT calling decoder.free() here because the getData/getChannel 
  // closures depend on it. In a real application, this might lead to memory leaks 
  // if not handled by FinalizationRegistry or similar.
  // For now, we prioritize functionality in tests.

  return { width, height, layers };
}

/**
 * Decode an EXR file expecting RGBA channels (optimized path).
 *
 * This is faster than decodeExr() when you know the image has RGBA channels.
 *
 * @param {Uint8Array} data - EXR file bytes
 * @returns {ExrRgbaDecodeResult} Decoded image data
 *
 * @example
 * await init();
 * const { width, height, data } = decodeExrRgba(bytes);
 */
export function decodeExrRgba(data: Uint8Array): ExrRgbaDecodeResult {
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
 * @returns {ExrRgbDecodeResult} Decoded image data
 *
 * @example
 * await init();
 * const { width, height, data } = decodeExrRgb(bytes);
 */
export function decodeExrRgb(data: Uint8Array): ExrRgbDecodeResult {
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
