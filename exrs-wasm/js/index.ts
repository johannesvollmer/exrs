/**
 * exrs - Typescript package for encoding and decoding EXR files in the browser and node.
 * Call init() once before using other functions.
 */
import * as wasm from 'exrs-raw-wasm-bindgen';
import shallow_equals from 'shallow-equals';

export type Precision = 'f16' | 'f32' | 'u32';
export type Compression = 'none' | 'rle' | 'zip' | 'zip16' | 'piz' | 'pxr24';
export type Channels = readonly string[];

export const RGBA: Channels = Object.freeze(['R', 'G', 'B', 'A']);
export const RGB: Channels = Object.freeze(['R', 'G', 'B']);

export interface ExrEncodeLayer {
  /** Layer name (e.g., "beauty", "depth") */
  name?: string;

  /**
   * Names of the channels in this layer.
   * @example RGBA, RGB, ['Z'], ['R', 'G', 'B', 'A'], ['X', 'Y', 'Z']
   */
  channelNames: Channels;

  /**
   * Contains all samples of all pixels.
   * Per pixel, one sample for each of the channelNames you specified, in the same order.
   */
  interleavedPixels: Float32Array;

  /**
   * Sample precision for this layer. applied to all channels.
   * @default 'f32'
   */
  precision?: Precision;

  /**
   * Compression method for this layer.
   * @default 'rle'
   */
  compression?: Compression;
}

/**
 * Multiple layers to be encoded into an EXR image.
 */
export interface ExrEncodeImage {
  width: number;
  height: number;
  layers: ExrEncodeLayer[];
}

/**
 * Options for encoding a single-layer RGBA EXR image.
 */
export interface ExrEncodeRgbaImage {
  width: number;
  height: number;

  /** Interleaved RGBA pixel data (R, G, B, A, R, G, B, A, ...) */
  interleavedRgbaPixels: Float32Array;

  /**
   * Sample precision for this layer. applied to all channels.
   * @default 'f32'
   */
  precision?: Precision;

  /**
   * Compression method for this layer.
   * @default 'rle'
   */
  compression?: Compression;
}

/**
 * Options for encoding a single-layer RGB EXR image.
 */
export interface ExrEncodeRgbImage {
  width: number;

  height: number;

  /** Interleaved RGB pixel data (R, G, B, R, G, B, ...) */
  interleavedRgbPixels: Float32Array;

  /**
   * Sample precision for this layer. applied to all channels.
   * @default 'f32'
   */
  precision?: Precision;

  /**
   * Compression method for this layer.
   * @default 'rle'
   */
  compression?: Compression;
}

export interface ExrDecodeLayer {
  name: string | null;

  /**
   * The names of all channels present in this layer, in alphabetical order.
   * EXR files always store channels alphabetically.
   */
  channelNamesAlphabetical: Channels;

  /**
   * Checks if the given channels are present in this layer, regardless of order.
   */
  containsChannelNames(channels: Channels): boolean;

  /**
   * Returns the pixel data for the specified channels, interleaved in the order you requested.
   * @example getInterleavedPixels(RGB), getInterleavedPixels(RGBA), getInterleavedPixels(["X", "Y", "Z"]), getInterleavedPixels(["B", "G", "R"])
   * @returns Float32Array or null if channels are not found
   */
  getInterleavedPixels(desiredChannels: Channels): Float32Array | null;

  /**
   * Returns all pixel data for all channels in this layer, interleaved in alphabetical order.
   */
  getAllInterleavedPixels(): Float32Array;
}

/**
 * Decoded EXR image containing one or more layers.
 */
export interface ExrDecodeImage {
  width: number;
  height: number;
  /** All layers found in the EXR file */
  layers: ExrDecodeLayer[];
}

/**
 * Decoded RGBA image data.
 */
export interface ExrDecodeRgbaImage {
  width: number;
  height: number;
  /** Interleaved RGBA pixel data */
  interleavedRgbaPixels: Float32Array;
}

/**
 * Decoded RGB image data.
 */
export interface ExrDecodeRgbImage {
  width: number;
  height: number;
  /** Interleaved RGB pixel data */
  interleavedRgbPixels: Float32Array;
}

// WASM module reference (set by init())
let isInit = false;

/**
 * Initialize the WASM module. Must be called once before using other functions.
 *
 * @example
 * import { init, encodeExr, decodeExr } from 'exrs';
 * await init();
 * // Now use encodeExr/decodeExr synchronously
 */
// Loads the binary *.wasm file
export async function init(): Promise<void> {
  if (isInit) return;

  // first, attempt browser style loading
  // otherwise try nodejs style (needed for testing)
  try {
    await wasm.default();
    isInit = true;
  } catch (e) {
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
      console.error('Failed to initialize EXRS WASM in both browser and Node environments');
      throw e;
    }
  }
}

function ensureInitialized() {
  if (!isInit) {
    throw new Error('EXRS WASM module not initialized. Call init() first.');
  }
}

/**
 * Encode an RGB image into an EXR file.
 * Specialized for performance.
 */
export function encodeRgbExr(image: ExrEncodeRgbImage): Uint8Array {
  return encodeExr({
    width: image.width,
    height: image.height,
    layers: [
      {
        channelNames: RGB,
        interleavedPixels: image.interleavedRgbPixels,
        compression: image.compression,
        precision: image.precision,
      },
    ],
  });
}

/**
 * Encode an RGBA image into an EXR file.
 * Specialized for performance.
 */
export function encodeRgbaExr(image: ExrEncodeRgbaImage): Uint8Array {
  return encodeExr({
    width: image.width,
    height: image.height,
    layers: [
      {
        channelNames: RGBA,
        interleavedPixels: image.interleavedRgbaPixels,
        compression: image.compression,
        precision: image.precision,
      },
    ],
  });
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

  const precisionByName: Record<Precision, wasm.SamplePrecision> = {
    f16: wasm.SamplePrecision.F16,
    f32: wasm.SamplePrecision.F32,
    u32: wasm.SamplePrecision.U32,
  };

  const compressionByName: Record<Compression, wasm.CompressionMethod> = {
    none: wasm.CompressionMethod.None,
    rle: wasm.CompressionMethod.Rle,
    zip: wasm.CompressionMethod.Zip,
    zip16: wasm.CompressionMethod.Zip16,
    piz: wasm.CompressionMethod.Piz,
    pxr24: wasm.CompressionMethod.Pxr24,
  };

  // special case: for plain old rgb(a) images,
  // we call specially optimized functions for performance
  if (layers.length === 1) {
    const layer = layers[0];
    const precision = precisionByName[layer.precision ?? 'f32'];
    const compression = compressionByName[layer.compression ?? 'rle'];

    if (shallow_equals(layer.channelNames, RGBA)) {
      return wasm.writeExrRgba(
        width,
        height,
        layer.name,
        layer.interleavedPixels,
        precision,
        compression,
      );
    } else if (shallow_equals(layer.channelNames, RGB)) {
      return wasm.writeExrRgb(
        width,
        height,
        layer.name,
        layer.interleavedPixels,
        precision,
        compression,
      );
    }
  }

  const encoder = new wasm.ExrEncoder(width, height);

  try {
    for (const layer of layers) {
      const precision = precisionByName[layer.precision ?? 'f32'];
      const compression = compressionByName[layer.compression ?? 'rle'];
      encoder.addLayer(
        layer.name,
        [...layer.channelNames],
        layer.interleavedPixels,
        precision,
        compression,
      );
    }

    return encoder.encode();
  } finally {
    // Free is optional with FinalizationRegistry, but we call it for deterministic cleanup
    encoder.free();
  }
}

/**
 * Decode an EXR file into pixel data.
 * You can call `.free()` on the return value for immediate cleanup,
 * but you can also leave it to the garbage collector.
 *
 * @example
 * await init();
 * const image = decodeExr(bytes);
 * console.log(image.width, image.height);
 *
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

      channelNamesAlphabetical: channels,

      containsChannelNames(desiredChannels: Channels): boolean {
        return desiredChannels.every((desired) => channels.includes(desired));
      },

      getInterleavedPixels: (desiredChannels: Channels): Float32Array | null => {
        return decoder.getLayerPixels(layerIndex, [...desiredChannels]) ?? null;
      },

      getAllInterleavedPixels: () => {
        const pixels = decoder.getLayerPixels(layerIndex, channels);
        if (!pixels) throw new Error("unreachable");
        return pixels;
      },
    });
  }

  // TODO add free: () => decoder.free()
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

  // it makes sense to specialize this here,
  // because it reduces wasm binding runtime cost
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

  // it makes sense to specialize this here,
  // because it reduces wasm binding runtime cost
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
