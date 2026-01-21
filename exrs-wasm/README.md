# exrs

WebAssembly bindings for reading and writing [OpenEXR](https://www.openexr.com/) files in the browser.

Built on top of the [`exr`](https://crates.io/crates/exr) Rust crate.

## Installation

```bash
npm install exrs
```

## Quick Start

```javascript
import { init, encodeExr, decodeExr } from 'exrs';

// Initialize WASM module (required once before using other functions)
await init();

// Create RGBA pixel data
const width = 1920;
const height = 1080;
const rgbaData = new Float64Array(width * height * 4);
// ... fill with pixel values ...

// Encode to EXR
const bytes = encodeExr({
  width,
  height,
  layers: [
    { name: 'beauty', channels: 'rgba', data: rgbaData, compression: 'piz' }
  ]
});

// Download or use the bytes
const blob = new Blob([bytes], { type: 'image/x-exr' });
```

## API

### `init()`

Initialize the WASM module. Must be called once before using other functions.

```typescript
await init();
```

### `encodeExr(options)`

Encode pixel data into an EXR file.

```typescript
const bytes = encodeExr({
  width: number,
  height: number,
  layers: [{
    name: string,                           // Layer name (e.g., "beauty")
    channels: 'rgba' | 'rgb' | string[],    // Channel type or custom names
    data: Float64Array,                     // Pixel data
    precision?: 'f16' | 'f32' | 'u32',      // Default: 'f32'
    compression?: 'none' | 'rle' | 'zip' | 'zip16' | 'piz' | 'pxr24'  // Default: 'rle'
  }]
});
```

### `decodeExr(data)`

Decode an EXR file into pixel data.

```typescript
const image = decodeExr(bytes);

console.log(image.width, image.height);
console.log(image.layers.length);

// Get pixel data (auto-detects RGBA/RGB/single channel based on layer contents)
const pixelData = image.layers[0].getData();

// Get individual channel by name
const depthData = image.layers[1].getChannel('Z');
```

### `decodeExrRgba(data)` / `decodeExrRgb(data)`

Optimized decoders for when you know the channel layout:

```typescript
const { width, height, data } = decodeExrRgba(bytes);
// data is interleaved RGBA Float64Array
```

## Examples

### Multi-layer EXR (AOVs)

```javascript
import { init, encodeExr } from 'exrs';

await init();

const bytes = encodeExr({
  width: 1920,
  height: 1080,
  layers: [
    { name: 'beauty', channels: 'rgba', data: beautyData, compression: 'piz' },
    { name: 'depth', channels: ['Z'], data: depthData, compression: 'pxr24' },
    { name: 'normals', channels: 'rgb', data: normalsData, compression: 'zip16' },
    { name: 'ao', channels: ['Y'], data: aoData, compression: 'rle' }
  ]
});
```

### WebGL Render Buffer Export

```javascript
import { init, encodeExr } from 'exrs';

await init();

// Read pixels from WebGL framebuffer
const gl = canvas.getContext('webgl2');
const pixels = new Float32Array(width * height * 4);
gl.readPixels(0, 0, width, height, gl.RGBA, gl.FLOAT, pixels);

// Convert to Float64Array (required by API)
const data = new Float64Array(pixels);

const bytes = encodeExr({
  width,
  height,
  layers: [{ name: 'render', channels: 'rgba', data, compression: 'piz' }]
});
```

### Load and Display EXR

```javascript
import { init, decodeExrRgba } from 'exrs';

await init();

const response = await fetch('image.exr');
const bytes = new Uint8Array(await response.arrayBuffer());

const { width, height, data } = decodeExrRgba(bytes);

// Display on canvas (tone mapping required for HDR)
const canvas = document.createElement('canvas');
canvas.width = width;
canvas.height = height;
const ctx = canvas.getContext('2d');
const imageData = ctx.createImageData(width, height);

for (let i = 0; i < width * height; i++) {
  // Simple tone mapping (Reinhard)
  const r = data[i * 4];
  const g = data[i * 4 + 1];
  const b = data[i * 4 + 2];
  const a = data[i * 4 + 3];

  imageData.data[i * 4] = Math.min(255, (r / (1 + r)) * 255);
  imageData.data[i * 4 + 1] = Math.min(255, (g / (1 + g)) * 255);
  imageData.data[i * 4 + 2] = Math.min(255, (b / (1 + b)) * 255);
  imageData.data[i * 4 + 3] = a * 255;
}

ctx.putImageData(imageData, 0, 0);
```

## Compression Methods

| Method | Description | Best For |
|--------|-------------|----------|
| `none` | No compression | Debug, speed |
| `rle` | Run-length encoding | Flat areas, fast |
| `zip` | ZIP (single scanline) | General purpose |
| `zip16` | ZIP (16 scanlines) | Good balance |
| `piz` | PIZ wavelet | Noisy images |
| `pxr24` | PXR24 | Depth buffers (lossy for f32) |

## Sample Precision

| Type | Description |
|------|-------------|
| `f16` | 16-bit half float (smaller files) |
| `f32` | 32-bit float (default, full precision) |
| `u32` | 32-bit unsigned integer |

## License

BSD-3-Clause

## Contributing

### Building and Testing

1. `cd js`
2. `npm run test` (this also rebuilds the whole wasm package every time)

### Building and Publishing

1. `cd js`
2. `npm run publish` (this also rebuilds the whole wasm package every time)
