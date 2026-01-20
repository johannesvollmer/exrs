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
   * @param type - 'rgba', 'rgb', or a channel name like 'Z'
   * @returns Pixel data as Float64Array, or null if channels don't exist
   */
  getData(type: 'rgba' | 'rgb' | string): Float64Array | null;
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
 * Encode pixel data into an EXR file.
 *
 * @param options - Encoding options including width, height, and layers
 * @returns EXR file bytes
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
export function encodeExr(options: ExrEncodeOptions): Promise<Uint8Array>;

/**
 * Decode an EXR file into pixel data.
 *
 * @param data - EXR file bytes
 * @returns Decoded image data with layer information
 *
 * @example
 * const image = await decodeExr(bytes);
 * const rgbaData = image.layers[0].getData('rgba');
 */
export function decodeExr(data: Uint8Array): Promise<ExrDecodeResult>;

/**
 * Decode an EXR file expecting RGBA channels (optimized path).
 * This is faster than decodeExr() when you know the image has RGBA channels.
 *
 * @param data - EXR file bytes
 * @returns Decoded RGBA image data
 */
export function decodeExrRgba(data: Uint8Array): Promise<ExrRgbaDecodeResult>;

/**
 * Decode an EXR file expecting RGB channels (optimized path).
 * This is faster than decodeExr() when you know the image has RGB channels.
 *
 * @param data - EXR file bytes
 * @returns Decoded RGB image data
 */
export function decodeExrRgb(data: Uint8Array): Promise<ExrRgbDecodeResult>;

/**
 * Initialize the WASM module (called automatically on first use).
 * You only need to call this if you want to pre-initialize before first use.
 */
export function init(): Promise<void>;
