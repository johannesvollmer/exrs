/**
 * exrs-wasm - JavaScript wrapper for writing and reading EXR files in the browser.
 *
 * This module provides a clean, easy-to-use API for EXR files.
 * Call init() once before using other functions.
 */
import * as wasm from "exrs-raw-wasm-bindgen"

export type Precision = 'f16' | 'f32' | 'u32';

export type Compression = 'none' | 'rle' | 'zip' | 'zip16' | 'piz' | 'pxr24';

export type Channels = readonly string[];

// TODO this is not so pretty...?
export const RGBA: Channels = Object.freeze("RGBA".split(''));
export const RGB: Channels = Object.freeze("RGB".split(''));

export interface ExrEncodeLayer {

  /** Layer name (e.g., "beauty", "depth") */
  name?: string;

  /** @example 'RGBA', 'RGB', ['Z'], ['R', 'G', 'B', 'A'], ['X', 'Y', 'Z'], ...  */
  channelNames: Channels;

  interleavedPixels: Float32Array;

  /** @default 'f32' */
  precision?: Precision;

  /** @default 'rle' */
  compression?: Compression;
}


export interface ExrEncodeImage {
  width: number;
  height: number;
  layers: ExrEncodeLayer[];
}

export interface ExrEncodeRgbaImage {
  width: number;
  height: number;
  interleavedRgbaPixels: Float32Array;

  /** @default 'f32' */
  precision?: Precision;

  /** @default 'rle' */
  compression?: Compression;
}

export interface ExrEncodeRgbImage {
  width: number;

  height: number;

  interleavedRgbPixels: Float32Array;

  /** @default 'f32' */
  precision?: Precision;

  /** @default 'rle' */
  compression?: Compression;
}

/** Decoded layer information */
export interface ExrDecodeLayer {
  name: string | null;

  /** Ordered alphabetically */
  channelNames: Channels;

  /** @example getInterleavedPixels(RGB), getInterleavedPixels(RGBA), getInterleavedPixels(["X", "Y", "Z"]) */
  getInterleavedPixels(desiredChannels: Channels): Float32Array | null;
  getAllInterleavedPixels(): Float32Array | null;
}

export interface ExrDecodeImage {
  width: number;
  height: number;
  layers: ExrDecodeLayer[];
}

export interface ExrDecodeRgbaImage {
  width: number;
  height: number;
  interleavedRgbaPixels: Float32Array;
}

export interface ExrDecodeRgbImage {
  width: number;
  height: number;
  interleavedRgbPixels: Float32Array;
}

// WASM module reference (set by init())
let isInit = false

/**
 * Initialize the WASM module. Must be called once before using other functions.
 *
 * @example
 * import { init, encodeExr, decodeExr } from 'exrs';
 * await init();
 * // Now use encodeExr/decodeExr synchronously
 */
export async function init(): Promise<void> {
  if (isInit) return;

  try {
    // Try browser-style loading
    await wasm.default();
    // FIXME is this really needed if we import it normally? or does the import run the default() function for us?
    isInit = true;

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

      wasm.initSync({ module: wasmBuffer });
      isInit = true;
    } catch (nodeErr) {
      console.error('Failed to initialize WASM in both browser and Node environments');
      throw e;
    }
  }
}

function ensureInitialized() {
  if (!isInit) {
    throw new Error('WASM module not initialized. Call init() first.');
  }
}

export function encodeRgbExr(image: ExrEncodeRgbImage): Uint8Array {
  return encodeExr({
    width: image.width,
    height: image.height,
    layers: [{
      channelNames: RGB,
      interleavedPixels: image.interleavedRgbPixels,
      compression: image.compression,
      precision: image.precision,
    }]
  })
}

export function encodeRgbaExr(image: ExrEncodeRgbaImage): Uint8Array {
  return encodeExr({
    width: image.width,
    height: image.height,
    layers: [{
      channelNames: RGBA,
      interleavedPixels: image.interleavedRgbaPixels,
      compression: image.compression,
      precision: image.precision,
    }]
  })
}

/**
 * Encode pixel data into an EXR file.
 *
 * @example
 * await init();
 * const bytes = encodeExr({
 *   width: 1920,
 *   height: 1080,
 *   layers: [
 *     { name: 'beauty', channelNames: 'rgba', interleavedPixels: rgbaData, precision: 'f32', compression: 'piz' },
 *     { name: 'depth', channelNames: ['Z'], interleavedPixels: depthData, precision: 'f32', compression: 'pxr24' }
 *   ]
 * });
 */
export function encodeExr(options: ExrEncodeImage): Uint8Array {
  ensureInitialized();

  const { width, height, layers } = options;

  // Map string precision/compression to WASM enum values
  const precisionMap: Record<Precision, any> = {
    f16: wasm.SamplePrecision.F16,
    f32: wasm.SamplePrecision.F32,
    u32: wasm.SamplePrecision.U32,
  };

  const compressionMap: Record<Compression, any> = {
    none: wasm.CompressionMethod.None,
    rle: wasm.CompressionMethod.Rle,
    zip: wasm.CompressionMethod.Zip,
    zip16: wasm.CompressionMethod.Zip16,
    piz: wasm.CompressionMethod.Piz,
    pxr24: wasm.CompressionMethod.Pxr24,
  };

  // Single layer shortcuts
  // TODO: This optimization could happen entirely in the rust file? would add more wasm bindgen overhead though
  if (layers.length === 1) {
    const layer = layers[0];
    const precision = precisionMap[layer.precision ?? 'f32'];
    const compression = compressionMap[layer.compression ?? 'rle'];

    if (layer.channelNames.join('') === 'rgba') {
      return wasm.writeExrRgba(width, height, layer.name, layer.interleavedPixels, precision, compression);
    } else if (layer.channelNames.join('') === 'rgb') {
      return wasm.writeExrRgb(width, height, layer.name, layer.interleavedPixels, precision, compression);
    }
  }

  const encoder = new wasm.ExrEncoder(width, height);

  try {
    for (const layer of layers) {
      const precision = precisionMap[layer.precision ?? 'f32'];
      const compression = compressionMap[layer.compression ?? 'rle'];
      encoder.addLayer(layer.name, [...layer.channelNames], layer.interleavedPixels, precision, compression);
    }

    return encoder.encode();
  } finally {
    // Free is optional with FinalizationRegistry, but we call it for deterministic cleanup
    encoder.free();
  }
}

/**
 * Decode an EXR file into pixel data.
 *
 * @example
 * await init();
 * const image = decodeExr(bytes);
 * console.log(image.width, image.height);
 *
 * // Get pixel data (auto-detects format based on channels)
 * const pixelData = image.layers[0].getInterleavedPixels();
 *
 * // Get a specific channel by name
 * const depthData = image.layers[1].getChannelPixels('Z');
 */
export function decodeExr(data: Uint8Array): ExrDecodeImage {
  ensureInitialized();

  const decoder = wasm.readExr(data);

  const width = decoder.width;
  const height = decoder.height;
  const layerCount = decoder.layerCount;

  const layers: ExrDecodeLayer[] = [];
  for (let layerIndex = 0; layerIndex < layerCount; layerIndex++) {
    const name = decoder.getLayerName(layerIndex) ?? null;
    const channels = decoder.getLayerChannelNames(layerIndex);

    layers.push({
      name,
      channelNames: channels,

      // Auto-detect format based on channel names
      getInterleavedPixels: (desiredChannels: Channels): Float32Array | null => {
        return decoder.getLayerPixels(layerIndex, [...desiredChannels]) ?? null
      },

      getAllInterleavedPixels: () => {
        return decoder.getLayerPixels(layerIndex, channels) ?? null;
      }
    });
  }

  // TODO add free: () => decoder.free()
  // Decoder is currently freed in rust when dropped, but wasm-bindgen handles it
  return { width, height, layers };
}

/**
 * Decode an EXR file expecting RGBA channels (optimized path).
 *
 * This is faster than decodeExr() when you know the image has RGBA channels.
 *
 * @param {Uint8Array} data - EXR file bytes
 * @returns {ExrDecodeRgbaImage} Decoded image data
 *
 * @example
 * await init();
 * const { width, height, interleavedRgbaPixels } = decodeRgbaExr(bytes);
 */
export function decodeRgbaExr(data: Uint8Array): ExrDecodeRgbaImage {
  ensureInitialized();

  // TODO this kind of optimizatoin/specialization could happen in the rust file, not here
  const result = wasm.readExrRgba(data);
  try {
    return {
      width: result.width,
      height: result.height,
      interleavedRgbaPixels: result.data,
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
 * @returns {ExrDecodeRgbImage} Decoded image data
 *
 * @example
 * await init();
 * const { width, height, interleavedRgbPixels } = decodeRgbExr(bytes);
 */
export function decodeRgbExr(data: Uint8Array): ExrDecodeRgbImage {
  ensureInitialized();

  // TODO this kind of optimizatoin/specialization could happen in the rust file, not here
  const result = wasm.readExrRgb(data);
  try {
    return {
      width: result.width,
      height: result.height,
      interleavedRgbPixels: result.data,
    };
  } finally {
    result.free();
  }
}
