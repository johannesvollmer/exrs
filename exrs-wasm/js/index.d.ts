/**
 * exrs-wasm - TypeScript definitions for writing and reading EXR files in the browser.
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

/**
 * Initialize the WASM module. Must be called before using other functions.
 *
 * @example
 * import { init, encodeExr, decodeExr } from 'exrs-wasm';
 * await init();
 * // Now use encodeExr/decodeExr synchronously
 */
export function init(): Promise<void>;

/**
 * Encode pixel data into an EXR file.
 * Call init() before using this function.
 *
 * @param options - Encoding options including width, height, and layers
 * @returns EXR file bytes
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
export function encodeExr(options: ExrEncodeOptions): Uint8Array;

/**
 * Decode an EXR file into pixel data.
 * Call init() before using this function.
 *
 * @param data - EXR file bytes
 * @returns Decoded image data with layer information
 *
 * @example
 * await init();
 * const image = decodeExr(bytes);
 * const pixelData = image.layers[0].getData();
 */
export function decodeExr(data: Uint8Array): ExrDecodeResult;

/**
 * Decode an EXR file expecting RGBA channels (optimized path).
 * This is faster than decodeExr() when you know the image has RGBA channels.
 * Call init() before using this function.
 *
 * @param data - EXR file bytes
 * @returns Decoded RGBA image data
 */
export function decodeExrRgba(data: Uint8Array): ExrRgbaDecodeResult;

/**
 * Decode an EXR file expecting RGB channels (optimized path).
 * This is faster than decodeExr() when you know the image has RGB channels.
 * Call init() before using this function.
 *
 * @param data - EXR file bytes
 * @returns Decoded RGB image data
 */
export function decodeExrRgb(data: Uint8Array): ExrRgbDecodeResult;
