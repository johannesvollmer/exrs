
import { describe, it, expect, beforeAll } from 'vitest';
import {
  init,
  encodeExr,
  encodeRgbaExr,
  encodeRgbExr,
  decodeExr,
  decodeRgbaExr,
  decodeRgbExr, RGB, RGBA
} from './index';

describe('EXRS WASM Integration Tests', () => {
  beforeAll(async () => {
    await init();
  });

  it('testBasicRgbaRoundtrip', () => {
    const width = 4;
    const height = 4;
    const pixelCount = width * height;

    // Create test RGBA data
    const data = new Float32Array(pixelCount * 4);
    for (let i = 0; i < pixelCount; i++) {
      data[i * 4] = i / pixelCount; // R
      data[i * 4 + 1] = 0.5; // G
      data[i * 4 + 2] = 0.25; // B
      data[i * 4 + 3] = 1.0; // A
    }

    // Encode
    const bytes = encodeRgbaExr({
      width,
      height,
      interleavedRgbaPixels: data,
      compression: 'none',
    });

    expect(bytes.length).toBeGreaterThan(0);
    expect(bytes[0]).toBe(0x76);
    expect(bytes[1]).toBe(0x2f);

    // Decode
    const image = decodeExr(bytes);
    expect(image.width).toBe(width);
    expect(image.height).toBe(height);
    expect(image.layers.length).toBe(1);

    const rgbaData = image.layers[0].getInterleavedPixels(RGBA);
    expect(rgbaData).not.toBeNull();
    if (rgbaData) {
      expect(rgbaData.length).toBe(pixelCount * 4);

      // Verify values
      for (let i = 0; i < pixelCount * 4; i++) {
        expect(rgbaData[i]).toBeCloseTo(data[i], 3);
      }
    }
  });

  it('testRgbRoundtrip', () => {
    const width = 8;
    const height = 8;
    const pixelCount = width * height;

    const data = new Float32Array(pixelCount * 3);
    for (let i = 0; i < pixelCount * 3; i++) {
      data[i] = i / 100;
    }

    const bytes = encodeRgbExr({
      width,
      height,
      interleavedRgbPixels: data,
      compression: 'rle',
    });

    const image = decodeExr(bytes);
    expect(image.layers.length).toBe(1);

    // getData() auto-detects RGB
    const rgbData = image.layers[0].getInterleavedPixels(RGB);
    expect(rgbData).not.toBeNull();
    if (rgbData) {
      expect(rgbData.length).toBe(pixelCount * 3);

      for (let i = 0; i < pixelCount * 3; i++) {
        expect(rgbData[i]).toBeCloseTo(data[i], 3);
      }
    }
  });

  it('testSingleChannelRoundtrip', () => {
    const width = 16;
    const height = 16;
    const pixelCount = width * height;

    const data = new Float32Array(pixelCount);
    for (let i = 0; i < pixelCount; i++) {
      data[i] = i;
    }

    const bytes = encodeExr({
      width,
      height,
      layers: [{ name: 'depth', channelNames: ['Z'], interleavedPixels: data, compression: 'piz' }],
    });

    const image = decodeExr(bytes);
    // getData() auto-detects single channel
    const zData = image.layers[0].getInterleavedPixels(["Z"]);
    expect(zData).not.toBeNull();
    if (zData) {
      expect(zData.length).toBe(pixelCount);

      for (let i = 0; i < pixelCount; i++) {
        expect(zData[i]).toBeCloseTo(data[i], 3);
      }
    }
  });

  it('testMultiLayer', () => {
    const width = 4;
    const height = 4;
    const pixelCount = width * height;

    const rgbaData = new Float32Array(pixelCount * 4).fill(0.8);
    const rgbData = new Float32Array(pixelCount * 3).fill(0.5);
    const depthData = new Float32Array(pixelCount).fill(1.0);

    const bytes = encodeExr({
      width,
      height,
      layers: [
        { name: 'beauty', channelNames: RGBA, interleavedPixels: rgbaData, compression: 'piz' },
        { name: 'normals', channelNames: RGB, interleavedPixels: rgbData, compression: 'zip16' },
        { name: 'depth', channelNames: ['Z'], interleavedPixels: depthData, compression: 'pxr24' },
      ],
    });

    const image = decodeExr(bytes);
    expect(image.layers.length).toBe(3);

    // Verify beauty layer (auto-detect RGBA)
    const beautyRgba = image.layers[0].getInterleavedPixels(RGBA);
    expect(beautyRgba).not.toBeNull();
    if (beautyRgba) {
      expect(beautyRgba.length).toBe(pixelCount * 4);
    }

    // Verify normals layer (auto-detect RGB)
    const normalsRgb = image.layers[1].getInterleavedPixels(RGB);
    expect(normalsRgb).not.toBeNull();
    if (normalsRgb) {
      expect(normalsRgb.length).toBe(pixelCount * 3);
    }

    // Verify depth layer (auto-detect single channel)
    const depthZ = image.layers[2].getInterleavedPixels(["Z"]);
    expect(depthZ).not.toBeNull();
    if (depthZ) {
      expect(depthZ.length).toBe(pixelCount);
    }
  });

  it('testGetChannel', () => {
    const width = 4;
    const height = 4;
    const pixelCount = width * height;

    const data = new Float32Array(pixelCount * 4);
    for (let i = 0; i < pixelCount; i++) {
      data[i * 4] = 0.1; // R
      data[i * 4 + 1] = 0.2; // G
      data[i * 4 + 2] = 0.3; // B
      data[i * 4 + 3] = 0.4; // A
    }

    const bytes = encodeRgbaExr({
      width,
      height,
      interleavedRgbaPixels: data,
    });

    const image = decodeExr(bytes);

    // get individual channels
    const rData = image.layers[0].getInterleavedPixels(['R']);
    const gData = image.layers[0].getInterleavedPixels(['G']);
    const bData = image.layers[0].getInterleavedPixels(['B']);
    const aData = image.layers[0].getInterleavedPixels(['A']);

    expect(rData).not.toBeNull();
    expect(gData).not.toBeNull();
    expect(bData).not.toBeNull();
    expect(aData).not.toBeNull();

    if (rData && gData && bData && aData) {
      expect(rData.length).toBe(pixelCount);
      expect(rData[0]).toBeCloseTo(0.1, 3);
      expect(gData[0]).toBeCloseTo(0.2, 3);
      expect(bData[0]).toBeCloseTo(0.3, 3);
      expect(aData[0]).toBeCloseTo(0.4, 3);
    }
  });

  it('testOptimizedRgbaRead', () => {
    const width = 4;
    const height = 4;
    const pixelCount = width * height;

    const data = new Float32Array(pixelCount * 4);
    for (let i = 0; i < pixelCount; i++) {
      data[i * 4] = i / pixelCount;
      data[i * 4 + 1] = 0.5;
      data[i * 4 + 2] = 0.25;
      data[i * 4 + 3] = 1.0;
    }

    const bytes = encodeRgbaExr({
      width,
      height,
      interleavedRgbaPixels: data,
    });

    // Use optimized RGBA reader
    const result = decodeRgbaExr(bytes);
    expect(result.width).toBe(width);
    expect(result.height).toBe(height);
    expect(result.interleavedRgbaPixels.length).toBe(pixelCount * 4);

    for (let i = 0; i < pixelCount * 4; i++) {
      expect(result.interleavedRgbaPixels[i]).toBeCloseTo(data[i], 3);
    }
  });

  it('testOptimizedRgbRead', () => {
    const width = 4;
    const height = 4;
    const pixelCount = width * height;

    const data = new Float32Array(pixelCount * 3);
    for (let i = 0; i < pixelCount * 3; i++) {
      data[i] = i / 100;
    }

    const bytes = encodeRgbExr({
      width,
      height,
      interleavedRgbPixels: data,
    });

    // Use optimized RGB reader
    const result = decodeRgbExr(bytes);
    expect(result.width).toBe(width);
    expect(result.height).toBe(height);
    expect(result.interleavedRgbPixels.length).toBe(pixelCount * 3);

    for (let i = 0; i < pixelCount * 3; i++) {
      expect(result.interleavedRgbPixels[i]).toBeCloseTo(data[i], 3);
    }
  });

  it('testCompressionMethods', () => {
    const width = 8;
    const height = 8;
    const pixelCount = width * height;
    const data = new Float32Array(pixelCount * 4).fill(0.5);

    const compressions = ['none', 'rle', 'zip', 'zip16', 'piz', 'pxr24'] as const;

    for (const compression of compressions) {
      const bytes = encodeRgbaExr({
        width,
        height,
        interleavedRgbaPixels: data,
        compression,
      });

      expect(bytes.length).toBeGreaterThan(0);

      const image = decodeExr(bytes);
      expect(image.width).toBe(width);
      expect(image.height).toBe(height);
    }
  });

  it('testF16Precision', () => {
    const width = 4;
    const height = 4;
    const pixelCount = width * height;
    const data = new Float32Array(pixelCount * 4).fill(0.5);

    const bytes = encodeRgbaExr({
      width,
      height,
      interleavedRgbaPixels: data,
      precision: 'f16',
    });

    const image = decodeExr(bytes);
    const rgbaData = image.layers[0].getInterleavedPixels(RGBA);
    expect(rgbaData).not.toBeNull();

    if (rgbaData) {
      // F16 has lower precision, allow larger epsilon
      for (let i = 0; i < pixelCount * 4; i++) {
        expect(rgbaData[i]).toBeCloseTo(data[i], 2);
      }
    }
  });

  it('testLayerNames', () => {
    const width = 2;
    const height = 2;
    const pixelCount = width * height;

    const bytes = encodeExr({
      width,
      height,
      layers: [
        { name: 'my_beauty', channelNames: RGBA, interleavedPixels: new Float32Array(pixelCount * 4) },
        { name: 'my_normals', channelNames: RGB, interleavedPixels: new Float32Array(pixelCount * 3) },
      ],
    });

    const image = decodeExr(bytes);
    expect(image.layers[0].name).toBe('my_beauty');
    expect(image.layers[1].name).toBe('my_normals');
  });

  it('testChannelNames', () => {
    const width = 2;
    const height = 2;
    const pixelCount = width * height;

    const bytes = encodeRgbaExr({
      width,
      height,
      interleavedRgbaPixels: new Float32Array(pixelCount * 4),
    });

    const image = decodeExr(bytes);
    const channels = image.layers[0].channelNames;

    expect(channels).toContain('R');
    expect(channels).toContain('G');
    expect(channels).toContain('B');
    expect(channels).toContain('A');
  });
});
