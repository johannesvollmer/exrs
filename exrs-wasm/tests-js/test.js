/**
 * JavaScript integration tests for exrs-wasm.
 *
 * Run with: npm test (after building with wasm-pack build --target web)
 */

import { encodeExr, decodeExr, decodeExrRgba, decodeExrRgb, init } from 'exrs-wasm';

// Simple assertion helper
function assert(condition, message) {
  if (!condition) {
    throw new Error(`Assertion failed: ${message}`);
  }
}

function assertClose(a, b, epsilon = 0.001, message = '') {
  if (Math.abs(a - b) >= epsilon) {
    throw new Error(`Assertion failed: ${a} !== ${b} (epsilon=${epsilon}) ${message}`);
  }
}

// Test results tracking
let passed = 0;
let failed = 0;

async function runTest(name, fn) {
  try {
    await fn();
    console.log(`  PASS: ${name}`);
    passed++;
  } catch (e) {
    console.error(`  FAIL: ${name}`);
    console.error(`    ${e.message}`);
    failed++;
  }
}

// Tests
async function testBasicRgbaRoundtrip() {
  const width = 4;
  const height = 4;
  const pixelCount = width * height;

  // Create test RGBA data
  const data = new Float64Array(pixelCount * 4);
  for (let i = 0; i < pixelCount; i++) {
    data[i * 4] = i / pixelCount; // R
    data[i * 4 + 1] = 0.5; // G
    data[i * 4 + 2] = 0.25; // B
    data[i * 4 + 3] = 1.0; // A
  }

  // Encode
  const bytes = await encodeExr({
    width,
    height,
    layers: [{ name: 'test', channels: 'rgba', data, compression: 'none' }],
  });

  assert(bytes.length > 0, 'EXR bytes should not be empty');
  assert(bytes[0] === 0x76 && bytes[1] === 0x2f, 'Should have EXR magic number');

  // Decode
  const image = await decodeExr(bytes);
  assert(image.width === width, `Width should be ${width}`);
  assert(image.height === height, `Height should be ${height}`);
  assert(image.layers.length === 1, 'Should have 1 layer');

  // Get RGBA data
  const rgbaData = image.layers[0].getData('rgba');
  assert(rgbaData.length === pixelCount * 4, 'RGBA data length mismatch');

  // Verify values
  for (let i = 0; i < pixelCount * 4; i++) {
    assertClose(data[i], rgbaData[i], 0.001, `at index ${i}`);
  }
}

async function testRgbRoundtrip() {
  const width = 8;
  const height = 8;
  const pixelCount = width * height;

  const data = new Float64Array(pixelCount * 3);
  for (let i = 0; i < pixelCount * 3; i++) {
    data[i] = i / 100;
  }

  const bytes = await encodeExr({
    width,
    height,
    layers: [{ name: 'normals', channels: 'rgb', data, compression: 'rle' }],
  });

  const image = await decodeExr(bytes);
  assert(image.layers.length === 1, 'Should have 1 layer');

  const rgbData = image.layers[0].getData('rgb');
  assert(rgbData.length === pixelCount * 3, 'RGB data length mismatch');

  for (let i = 0; i < pixelCount * 3; i++) {
    assertClose(data[i], rgbData[i], 0.001, `at index ${i}`);
  }
}

async function testSingleChannelRoundtrip() {
  const width = 16;
  const height = 16;
  const pixelCount = width * height;

  const data = new Float64Array(pixelCount);
  for (let i = 0; i < pixelCount; i++) {
    data[i] = i;
  }

  const bytes = await encodeExr({
    width,
    height,
    layers: [{ name: 'depth', channels: ['Z'], data, compression: 'piz' }],
  });

  const image = await decodeExr(bytes);
  const zData = image.layers[0].getData('Z');
  assert(zData.length === pixelCount, 'Z data length mismatch');

  for (let i = 0; i < pixelCount; i++) {
    assertClose(data[i], zData[i], 0.001, `at index ${i}`);
  }
}

async function testMultiLayer() {
  const width = 4;
  const height = 4;
  const pixelCount = width * height;

  const rgbaData = new Float64Array(pixelCount * 4).fill(0.8);
  const rgbData = new Float64Array(pixelCount * 3).fill(0.5);
  const depthData = new Float64Array(pixelCount).fill(1.0);

  const bytes = await encodeExr({
    width,
    height,
    layers: [
      { name: 'beauty', channels: 'rgba', data: rgbaData, compression: 'piz' },
      { name: 'normals', channels: 'rgb', data: rgbData, compression: 'zip16' },
      { name: 'depth', channels: ['Z'], data: depthData, compression: 'pxr24' },
    ],
  });

  const image = await decodeExr(bytes);
  assert(image.layers.length === 3, 'Should have 3 layers');

  // Verify beauty layer
  const beautyRgba = image.layers[0].getData('rgba');
  assert(beautyRgba.length === pixelCount * 4, 'Beauty RGBA length mismatch');

  // Verify normals layer
  const normalsRgb = image.layers[1].getData('rgb');
  assert(normalsRgb.length === pixelCount * 3, 'Normals RGB length mismatch');

  // Verify depth layer
  const depthZ = image.layers[2].getData('Z');
  assert(depthZ.length === pixelCount, 'Depth Z length mismatch');
}

async function testOptimizedRgbaRead() {
  const width = 4;
  const height = 4;
  const pixelCount = width * height;

  const data = new Float64Array(pixelCount * 4);
  for (let i = 0; i < pixelCount; i++) {
    data[i * 4] = i / pixelCount;
    data[i * 4 + 1] = 0.5;
    data[i * 4 + 2] = 0.25;
    data[i * 4 + 3] = 1.0;
  }

  const bytes = await encodeExr({
    width,
    height,
    layers: [{ name: 'test', channels: 'rgba', data }],
  });

  // Use optimized RGBA reader
  const result = await decodeExrRgba(bytes);
  assert(result.width === width, 'Width mismatch');
  assert(result.height === height, 'Height mismatch');
  assert(result.data.length === pixelCount * 4, 'Data length mismatch');

  for (let i = 0; i < pixelCount * 4; i++) {
    assertClose(data[i], result.data[i], 0.001, `at index ${i}`);
  }
}

async function testOptimizedRgbRead() {
  const width = 4;
  const height = 4;
  const pixelCount = width * height;

  const data = new Float64Array(pixelCount * 3);
  for (let i = 0; i < pixelCount * 3; i++) {
    data[i] = i / 100;
  }

  const bytes = await encodeExr({
    width,
    height,
    layers: [{ name: 'normals', channels: 'rgb', data }],
  });

  // Use optimized RGB reader
  const result = await decodeExrRgb(bytes);
  assert(result.width === width, 'Width mismatch');
  assert(result.height === height, 'Height mismatch');
  assert(result.data.length === pixelCount * 3, 'Data length mismatch');

  for (let i = 0; i < pixelCount * 3; i++) {
    assertClose(data[i], result.data[i], 0.001, `at index ${i}`);
  }
}

async function testCompressionMethods() {
  const width = 8;
  const height = 8;
  const pixelCount = width * height;
  const data = new Float64Array(pixelCount * 4).fill(0.5);

  const compressions = ['none', 'rle', 'zip', 'zip16', 'piz', 'pxr24'];

  for (const compression of compressions) {
    const bytes = await encodeExr({
      width,
      height,
      layers: [{ name: 'test', channels: 'rgba', data, compression }],
    });

    assert(bytes.length > 0, `${compression} should produce bytes`);

    const image = await decodeExr(bytes);
    assert(image.width === width, `${compression}: width mismatch`);
    assert(image.height === height, `${compression}: height mismatch`);
  }
}

async function testF16Precision() {
  const width = 4;
  const height = 4;
  const pixelCount = width * height;
  const data = new Float64Array(pixelCount * 4).fill(0.5);

  const bytes = await encodeExr({
    width,
    height,
    layers: [{ name: 'test', channels: 'rgba', data, precision: 'f16' }],
  });

  const image = await decodeExr(bytes);
  const rgbaData = image.layers[0].getData('rgba');

  // F16 has lower precision, allow larger epsilon
  for (let i = 0; i < pixelCount * 4; i++) {
    assertClose(data[i], rgbaData[i], 0.01, `F16 precision at index ${i}`);
  }
}

async function testLayerNames() {
  const width = 2;
  const height = 2;
  const pixelCount = width * height;

  const bytes = await encodeExr({
    width,
    height,
    layers: [
      { name: 'my_beauty', channels: 'rgba', data: new Float64Array(pixelCount * 4) },
      { name: 'my_normals', channels: 'rgb', data: new Float64Array(pixelCount * 3) },
    ],
  });

  const image = await decodeExr(bytes);
  assert(image.layers[0].name === 'my_beauty', 'First layer name mismatch');
  assert(image.layers[1].name === 'my_normals', 'Second layer name mismatch');
}

async function testChannelNames() {
  const width = 2;
  const height = 2;
  const pixelCount = width * height;

  const bytes = await encodeExr({
    width,
    height,
    layers: [{ name: 'test', channels: 'rgba', data: new Float64Array(pixelCount * 4) }],
  });

  const image = await decodeExr(bytes);
  const channels = image.layers[0].channels;

  assert(channels.includes('R'), 'Should have R channel');
  assert(channels.includes('G'), 'Should have G channel');
  assert(channels.includes('B'), 'Should have B channel');
  assert(channels.includes('A'), 'Should have A channel');
}

// Main
async function main() {
  console.log('exrs-wasm JS Integration Tests\n');

  // Initialize WASM module
  await init();

  console.log('Running tests...\n');

  await runTest('Basic RGBA roundtrip', testBasicRgbaRoundtrip);
  await runTest('RGB roundtrip', testRgbRoundtrip);
  await runTest('Single channel roundtrip', testSingleChannelRoundtrip);
  await runTest('Multi-layer EXR', testMultiLayer);
  await runTest('Optimized RGBA read', testOptimizedRgbaRead);
  await runTest('Optimized RGB read', testOptimizedRgbRead);
  await runTest('Compression methods', testCompressionMethods);
  await runTest('F16 precision', testF16Precision);
  await runTest('Layer names', testLayerNames);
  await runTest('Channel names', testChannelNames);

  console.log(`\nResults: ${passed} passed, ${failed} failed`);

  if (failed > 0) {
    process.exit(1);
  }
}

main().catch((e) => {
  console.error('Test runner error:', e);
  process.exit(1);
});
