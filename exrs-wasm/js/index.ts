/**
 * exrs-wasm - JavaScript wrapper for writing and reading EXR files in the browser.
 *
 * This module provides a clean, easy-to-use API for EXR files.
 * Call init() once before using other functions.
 */


export type Precision = 'f16' | 'f32' | 'u32';

export type Compression = 'none' | 'rle' | 'zip' | 'zip16' | 'piz' | 'pxr24';

/** You can always use string[]. But use rgb or rgba directly for simplicity and speed. */
export type Channels = 'rgba' | 'rgb' | string[];

export interface ExrEncodeLayer {

  /** Layer name (e.g., "beauty", "depth") */
  name?: string;

  /** @example 'rgba', 'rgb', or ['Z'] */
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

  channelNames: string[];

  getInterleavedPixels(): Float32Array;

  /**
   * Get all samples for a specific channel by name.
   * @param channelName - e.g. 'R', 'G', 'B', 'A', 'Z'
   */
  getChannelPixels(channelName: string): Float32Array | null;
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
let wasmModule: any = null;

/**
 * Initialize the WASM module. Must be called once before using other functions.
 *
 * @example
 * import { init, encodeExr, decodeExr } from 'exrs';
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
      wasm.initSync({ module: wasmBuffer });
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

export function encodeRgbExr(image: ExrEncodeRgbImage): Uint8Array {
  return encodeExr({
    width: image.width,
    height: image.height,
    layers: [{
      channelNames: "rgb",
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
      channelNames: "rgba",
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
  // TODO: This optimization should happen entirely in the rust file
  if (layers.length === 1) {
    const layer = layers[0];
    const precision = precisionMap[layer.precision || 'f32'];
    const compression = compressionMap[layer.compression || 'rle'];

    if (layer.channelNames === 'rgba') {
      return wasmModule.writeExrRgba(width, height, layer.name, layer.interleavedPixels, precision, compression);
    } else if (layer.channelNames === 'rgb') {
      return wasmModule.writeExrRgb(width, height, layer.name, layer.interleavedPixels, precision, compression);
    } else if (Array.isArray(layer.channelNames) && layer.channelNames.length === 1) {
      return wasmModule.writeExrSingleChannel(
        width, height, layer.name, layer.channelNames[0], layer.interleavedPixels, precision, compression
      );
    }
  }

  // Multi-layer: use the encoder
  const encoder = new wasmModule.ExrEncoder(width, height);

  try {
    for (const layer of layers) {
      const precision = precisionMap[layer.precision || 'f32'];
      const compression = compressionMap[layer.compression || 'rle'];

      if (layer.channelNames === 'rgba') {
        encoder.addRgbaLayer(layer.name, layer.interleavedPixels, precision, compression);
      } else if (layer.channelNames === 'rgb') {
        encoder.addRgbLayer(layer.name, layer.interleavedPixels, precision, compression);
      } else if (Array.isArray(layer.channelNames) && layer.channelNames.length === 1) {
        encoder.addSingleChannelLayer(layer.name, layer.channelNames[0], layer.interleavedPixels, precision, compression);
      } else {
        throw new Error(`Unsupported channels format: ${JSON.stringify(layer.channelNames)}`);
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

  const decoder = wasmModule.readExr(data);

  const width = decoder.width;
  const height = decoder.height;
  const layerCount = decoder.layerCount;

  const layers: ExrDecodeLayer[] = [];
  for (let i = 0; i < layerCount; i++) {
    const name = decoder.getLayerName(i) ?? null;

    const channels = decoder.getChannelNames(i);

    // Determine channel type from actual channel names
    // TODO we should intentionally decide what happens with layers that are RGBZ. is this a use case?
    const hasR = channels.includes('R');
    const hasG = channels.includes('G');
    const hasB = channels.includes('B');
    const hasA = channels.includes('A');
    const isRgba = hasR && hasG && hasB && hasA;
    const isRgb = hasR && hasG && hasB && !hasA;

    layers.push({
      name,
      channelNames: channels,
      // Auto-detect format based on channel names
      getInterleavedPixels: (): Float32Array => {
        if (isRgba) {
          return decoder.getRgbaData(i);
        } else if (isRgb) {
          // TODO this can be undefined, why does TS not show a type mismatch here?!
          return decoder.getRgbData(i);
        } else if (channels.length === 1) {
          return decoder.getChannelData(i, channels[0]);
        } else {
          // TODO: add one generic function in rust that just interleaves all the desired/existing channels
          throw new Error("multiple channels not supported yet");
        }
      },

      getChannelPixels: (channelName: string) =>
        decoder.getChannelData(i, channelName),
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
  const result = wasmModule.readExrRgba(data);
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
  const result = wasmModule.readExrRgb(data);
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
