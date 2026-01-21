/**
 * JavaScript integration tests for exrs-wasm.
 *
 * Run with: npm test (after building with wasm-pack build --target web)
 */

import { init, encodeExr, decodeExr, decodeExrRgba, decodeExrRgb } from 'exrs-wasm';

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

function runTest(name, fn) {
  try {
    fn();
    console.log(`  PASS: ${name}`);
    passed++;
  } catch (e) {
    console.error(`  FAIL: ${name}`);
    console.error(`    ${e.message}`);
    failed++;
  }
}

// Tests
function testBasicRgbaRoundtrip() {
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
  const bytes = encodeExr({
    width,
    height,
    layers: [{ name: 'test', channels: 'rgba', data, compression: 'none' }],
  });

  assert(bytes.length > 0, 'EXR bytes should not be empty');
  assert(bytes[0] === 0x76 && bytes[1] === 0x2f, 'Should have EXR magic number');

  // Decode
  const image = decodeExr(bytes);
  assert(image.width === width, `Width should be ${width}`);
  assert(image.height === height, `Height should be ${height}`);
  assert(image.layers.length === 1, 'Should have 1 layer');

  // Get RGBA data (auto-detected)
  const rgbaData = image.layers[0].getData();
  assert(rgbaData.length === pixelCount * 4, 'RGBA data length mismatch');

  // Verify values
  for (let i = 0; i < pixelCount * 4; i++) {
    assertClose(data[i], rgbaData[i], 0.001, `at index ${i}`);
  }
}

function testRgbRoundtrip() {
  const width = 8;
  const height = 8;
  const pixelCount = width * height;

  const data = new Float64Array(pixelCount * 3);
  for (let i = 0; i < pixelCount * 3; i++) {
    data[i] = i / 100;
  }

  const bytes = encodeExr({
    width,
    height,
    layers: [{ name: 'normals', channels: 'rgb', data, compression: 'rle' }],
  });

  const image = decodeExr(bytes);
  assert(image.layers.length === 1, 'Should have 1 layer');

  // getData() auto-detects RGB
  const rgbData = image.layers[0].getData();
  assert(rgbData.length === pixelCount * 3, 'RGB data length mismatch');

  for (let i = 0; i < pixelCount * 3; i++) {
    assertClose(data[i], rgbData[i], 0.001, `at index ${i}`);
  }
}

function testSingleChannelRoundtrip() {
  const width = 16;
  const height = 16;
  const pixelCount = width * height;

  const data = new Float64Array(pixelCount);
  for (let i = 0; i < pixelCount; i++) {
    data[i] = i;
  }

  const bytes = encodeExr({
    width,
    height,
    layers: [{ name: 'depth', channels: ['Z'], data, compression: 'piz' }],
  });

  const image = decodeExr(bytes);
  // getData() auto-detects single channel
  const zData = image.layers[0].getData();
  assert(zData.length === pixelCount, 'Z data length mismatch');

  for (let i = 0; i < pixelCount; i++) {
    assertClose(data[i], zData[i], 0.001, `at index ${i}`);
  }
}

function testMultiLayer() {
  const width = 4;
  const height = 4;
  const pixelCount = width * height;

  const rgbaData = new Float64Array(pixelCount * 4).fill(0.8);
  const rgbData = new Float64Array(pixelCount * 3).fill(0.5);
  const depthData = new Float64Array(pixelCount).fill(1.0);

  const bytes = encodeExr({
    width,
    height,
    layers: [
      { name: 'beauty', channels: 'rgba', data: rgbaData, compression: 'piz' },
      { name: 'normals', channels: 'rgb', data: rgbData, compression: 'zip16' },
      { name: 'depth', channels: ['Z'], data: depthData, compression: 'pxr24' },
    ],
  });

  const image = decodeExr(bytes);
  assert(image.layers.length === 3, 'Should have 3 layers');

  // Verify beauty layer (auto-detect RGBA)
  const beautyRgba = image.layers[0].getData();
  assert(beautyRgba.length === pixelCount * 4, 'Beauty RGBA length mismatch');

  // Verify normals layer (auto-detect RGB)
  const normalsRgb = image.layers[1].getData();
  assert(normalsRgb.length === pixelCount * 3, 'Normals RGB length mismatch');

  // Verify depth layer (auto-detect single channel)
  const depthZ = image.layers[2].getData();
  assert(depthZ.length === pixelCount, 'Depth Z length mismatch');
}

function testGetChannel() {
  const width = 4;
  const height = 4;
  const pixelCount = width * height;

  const data = new Float64Array(pixelCount * 4);
  for (let i = 0; i < pixelCount; i++) {
    data[i * 4] = 0.1; // R
    data[i * 4 + 1] = 0.2; // G
    data[i * 4 + 2] = 0.3; // B
    data[i * 4 + 3] = 0.4; // A
  }

  const bytes = encodeExr({
    width,
    height,
    layers: [{ name: 'test', channels: 'rgba', data }],
  });

  const image = decodeExr(bytes);

  // Use getChannel to get individual channels
  const rData = image.layers[0].getChannel('R');
  const gData = image.layers[0].getChannel('G');
  const bData = image.layers[0].getChannel('B');
  const aData = image.layers[0].getChannel('A');

  assert(rData.length === pixelCount, 'R channel length mismatch');
  assertClose(rData[0], 0.1, 0.001, 'R value');
  assertClose(gData[0], 0.2, 0.001, 'G value');
  assertClose(bData[0], 0.3, 0.001, 'B value');
  assertClose(aData[0], 0.4, 0.001, 'A value');
}

function testOptimizedRgbaRead() {
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

  const bytes = encodeExr({
    width,
    height,
    layers: [{ name: 'test', channels: 'rgba', data }],
  });

  // Use optimized RGBA reader
  const result = decodeExrRgba(bytes);
  assert(result.width === width, 'Width mismatch');
  assert(result.height === height, 'Height mismatch');
  assert(result.data.length === pixelCount * 4, 'Data length mismatch');

  for (let i = 0; i < pixelCount * 4; i++) {
    assertClose(data[i], result.data[i], 0.001, `at index ${i}`);
  }
}

function testOptimizedRgbRead() {
  const width = 4;
  const height = 4;
  const pixelCount = width * height;

  const data = new Float64Array(pixelCount * 3);
  for (let i = 0; i < pixelCount * 3; i++) {
    data[i] = i / 100;
  }

  const bytes = encodeExr({
    width,
    height,
    layers: [{ name: 'normals', channels: 'rgb', data }],
  });

  // Use optimized RGB reader
  const result = decodeExrRgb(bytes);
  assert(result.width === width, 'Width mismatch');
  assert(result.height === height, 'Height mismatch');
  assert(result.data.length === pixelCount * 3, 'Data length mismatch');

  for (let i = 0; i < pixelCount * 3; i++) {
    assertClose(data[i], result.data[i], 0.001, `at index ${i}`);
  }
}

function testCompressionMethods() {
  const width = 8;
  const height = 8;
  const pixelCount = width * height;
  const data = new Float64Array(pixelCount * 4).fill(0.5);

  const compressions = ['none', 'rle', 'zip', 'zip16', 'piz', 'pxr24'];

  for (const compression of compressions) {
    const bytes = encodeExr({
      width,
      height,
      layers: [{ name: 'test', channels: 'rgba', data, compression }],
    });

    assert(bytes.length > 0, `${compression} should produce bytes`);

    const image = decodeExr(bytes);
    assert(image.width === width, `${compression}: width mismatch`);
    assert(image.height === height, `${compression}: height mismatch`);
  }
}

function testF16Precision() {
  const width = 4;
  const height = 4;
  const pixelCount = width * height;
  const data = new Float64Array(pixelCount * 4).fill(0.5);

  const bytes = encodeExr({
    width,
    height,
    layers: [{ name: 'test', channels: 'rgba', data, precision: 'f16' }],
  });

  const image = decodeExr(bytes);
  const rgbaData = image.layers[0].getData();

  // F16 has lower precision, allow larger epsilon
  for (let i = 0; i < pixelCount * 4; i++) {
    assertClose(data[i], rgbaData[i], 0.01, `F16 precision at index ${i}`);
  }
}

function testLayerNames() {
  const width = 2;
  const height = 2;
  const pixelCount = width * height;

  const bytes = encodeExr({
    width,
    height,
    layers: [
      { name: 'my_beauty', channels: 'rgba', data: new Float64Array(pixelCount * 4) },
      { name: 'my_normals', channels: 'rgb', data: new Float64Array(pixelCount * 3) },
    ],
  });

  const image = decodeExr(bytes);
  assert(image.layers[0].name === 'my_beauty', 'First layer name mismatch');
  assert(image.layers[1].name === 'my_normals', 'Second layer name mismatch');
}

function testChannelNames() {
  const width = 2;
  const height = 2;
  const pixelCount = width * height;

  const bytes = encodeExr({
    width,
    height,
    layers: [{ name: 'test', channels: 'rgba', data: new Float64Array(pixelCount * 4) }],
  });

  const image = decodeExr(bytes);
  const channels = image.layers[0].channels;

  assert(channels.includes('R'), 'Should have R channel');
  assert(channels.includes('G'), 'Should have G channel');
  assert(channels.includes('B'), 'Should have B channel');
  assert(channels.includes('A'), 'Should have A channel');
}

// Main
async function main() {
  console.log('exrs-wasm JS Integration Tests\n');

  // Initialize WASM module (required before using other functions)
  await init();

  console.log('Running tests...\n');

  runTest('Basic RGBA roundtrip', testBasicRgbaRoundtrip);
  runTest('RGB roundtrip', testRgbRoundtrip);
  runTest('Single channel roundtrip', testSingleChannelRoundtrip);
  runTest('Multi-layer EXR', testMultiLayer);
  runTest('getChannel() for specific channels', testGetChannel);
  runTest('Optimized RGBA read', testOptimizedRgbaRead);
  runTest('Optimized RGB read', testOptimizedRgbRead);
  runTest('Compression methods', testCompressionMethods);
  runTest('F16 precision', testF16Precision);
  runTest('Layer names', testLayerNames);
  runTest('Channel names', testChannelNames);

  console.log(`\nResults: ${passed} passed, ${failed} failed`);

  if (failed > 0) {
    process.exit(1);
  }
}

main().catch((e) => {
  console.error('Test runner error:', e);
  process.exit(1);
});
