# exrs-wasm

WebAssembly bindings for reading and writing [OpenEXR](https://www.openexr.com/) files in the browser.

Built on top of the [`exrs`](https://github.com/johannesvollmer/exrs) Rust crate.

## Installation

```bash
npm install exrs
```

## Quick Start

```javascript
import { init, encodeExr, decodeRgbaExr } from 'exrs-wasm';

// Initialize WASM module (required once before using other functions)
await init();

// Create RGBA pixel data
const width = 1920;
const height = 1080;
const interleavedPixels = new Float32Array(width * height * 4);
// ... fill with pixel values ...

// Encode to EXR
const bytes: Uint8Array = encodeExr({
    width,
    height,
    layers: [{
        channelNames: 'rgba',
        interleavedPixels,
        compression: 'piz'
    }]
});

// Decode back
const { width: w, height: h, interleavedRgbaPixels } = decodeRgbaExr(bytes);
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
  width: 1920,
  height: 1080,
  layers: [{
    name: 'beauty',                                 // Layer name (e.g., "beauty")
    channelNames: RGBA,                           // 'rgba', 'rgb' or string[]
    interleavedPixels: rgbaData,                    // Float32Array
    precision: 'f32',                               // 'f16', 'f32', or 'u32'
    compression: 'piz'                              // 'none', 'rle', 'zip', 'zip16', 'piz', 'pxr24'
  }]
});
```

### `decodeExr(data)`

Decode an EXR file into pixel data.

```typescript
const image = decodeExr(bytes);

console.log(image.width, image.height);
console.log(image.layers.length);

const pixelData = image.layers[0].getAllInterleavedPixels();

// Get individual channel by name
const depthData = image.layers[1].getInterleavedPixels(['Z']);
```

### `decodeRgbaExr(data)` / `decodeRgbExr(data)`

Optimized decoders for when you know the channel layout:

```typescript
const { width, height, interleavedRgbaPixels } = decodeRgbaExr(bytes);
// interleavedRgbaPixels is interleaved RGBA Float32Array
```

## Examples

### Multi-layer EXR (AOVs)

```javascript
import { init, encodeExr } from 'exrs-wasm';

await init();

const bytes = encodeExr({
  width: 1920,
  height: 1080,
  layers: [
    { name: 'beauty', channelNames: 'rgba', interleavedPixels: beautyData, compression: 'piz' },
    { name: 'depth', channelNames: ['Z'], interleavedPixels: depthData, compression: 'pxr24' },
    { name: 'normals', channelNames: 'rgb', interleavedPixels: normalsData, compression: 'zip16' },
    { name: 'ao', channelNames: ['Y'], interleavedPixels: aoData, compression: 'rle' }
  ]
});
```

### WebGL Render Buffer Export

```javascript
import { init, encodeExr } from 'exrs-wasm';

await init();

// Read pixels from WebGL framebuffer
const gl = canvas.getContext('webgl2');
const pixels = new Float32Array(width * height * 4);
gl.readPixels(0, 0, width, height, gl.RGBA, gl.FLOAT, pixels);

const bytes = encodeExr({
  width,
  height,
  layers: [{ name: 'render', channelNames: 'rgba', interleavedPixels: pixels, compression: 'piz' }]
});
```

### Load and Display EXR

```javascript
import { init, decodeRgbaExr } from 'exrs-wasm';

await init();

const response = await fetch('image.exr');
const bytes = new Uint8Array(await response.arrayBuffer());

const { width, height, interleavedRgbaPixels } = decodeRgbaExr(bytes);

// Display on canvas (tone mapping required for HDR)
const canvas = document.createElement('canvas');
canvas.width = width;
canvas.height = height;
const ctx = canvas.getContext('2d');
const imageData = ctx.createImageData(width, height);

for (let i = 0; i < width * height; i++) {
  // Simple tone mapping (Reinhard)
  const r = interleavedRgbaPixels[i * 4];
  const g = interleavedRgbaPixels[i * 4 + 1];
  const b = interleavedRgbaPixels[i * 4 + 2];
  const a = interleavedRgbaPixels[i * 4 + 3];

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

Prerequisites:
- You will need Rust `cargo`, `npm` and `nodejs`
- `cargo install wasm-pack`

### Building and Testing

1. `cd js`
2. `npm run test` (this also rebuilds the whole wasm package every time)

### Building and Publishing

1. `cd js`
2. `npm run publish` (this also rebuilds the whole wasm package every time)
