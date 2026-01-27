# exrs

WebAssembly bindings for reading and writing [OpenEXR](https://www.openexr.com/) files in the browser and Node.js.

Built on top of the [`exrs`](https://github.com/johannesvollmer/exrs) Rust crate.

## Features

- **Fast**: Specialized paths for RGB/RGBA images.
- **Flexible**: Support for multiple layers and custom channels.
- **Modern**: Clean TypeScript/JavaScript API with standard `Float32Array`.
- **Portable**: Works in Browser and Node.js.
- **Compatible**: Supports various compression methods and bit depths.

## Installation

```bash
npm install exrs
```

## Examples

### Encoding

#### `encodeExr(options: ExrEncodeImage): Uint8Array`
The most flexible encoding function, supporting multiple layers and arbitrary channel names.
```typescript
import {init, encodeExr} from "exrs";
await init();

const bytes = encodeExr({
  width: 1920,
  height: 1080,
  layers: [{
    name: 'beauty',
    channelNames: ['R', 'G', 'B', 'A'],
    interleavedPixels: rgbaData,
    precision: 'f32',
    compression: 'piz'
  }]
});
```

#### `encodeRgbaExr(image: ExrEncodeRgbaImage): Uint8Array`, `encodeRgbExr(image: ExrEncodeRgbImage): Uint8Array`
Optimized encoders for standard RGB or RGBA channel images.

```typescript
import {init, encodeRgbaExr} from "exrs";
await init();

const bytes = encodeRgbaExr({
    width: 1920,
    height: 1080,
    interleavedPixels: rgbaData,
    precision: 'f32',
    compression: 'piz'
});
```


### Decoding

#### `decodeExr(data: Uint8Array): ExrDecodeImage`
Decodes an EXR file into a structured image object containing one or more layers. Most flexible decoding function.
```typescript
import {init, decodeExr} from "exrs";
await init();

const image = decodeExr(bytes);
const layer = image.layers[0];

// Get pixels in a specific order
const pixels = layer.getInterleavedPixels(['R', 'G', 'B']);

// Check for specific channels
if (layer.containsChannelNames(['Z'])) {
    const depth = layer.getInterleavedPixels(['Z']);
}
```

#### `decodeRgbaExr(data: Uint8Array): ExrDecodeRgbaImage`, `decodeRgbExr(data: Uint8Array): ExrDecodeRgbImage`
High-performance decoders that return a simple object with width, height, and a single `Float32Array` of interleaved pixels.

```typescript
import {init, decodeRgbaExr} from "exrs";
await init();

const image = decodeRgbaExr(bytes);
const { width, height } = image;
const pixels = image.interleavedRgbaPixels;
```

### Layer Options

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `name` | `string` | `undefined` | Optional name for the layer |
| `channelNames` | `string[]` | **Required** | Names of channels (e.g., `['R', 'G', 'B']`) |
| `interleavedPixels`| `Float32Array` | **Required** | Pixel data interleaved by channel |
| `precision` | `'f16' \| 'f32' \| 'u32'` | `'f32'` | Sample bit depth |
| `compression` | `Compression` | `'rle'` | Compression method |

### Compression Methods

| Method | Description | Best For |
|--------|-------------|----------|
| `none` | No compression | Debugging, extreme speed |
| `rle` | Run-length encoding | Flat/simple images, fast |
| `zip` | ZIP (single scanline) | General purpose |
| `zip16` | ZIP (16 scanlines) | Good balance |
| `piz` | PIZ wavelet | Noisy/natural images (often best) |
| `pxr24` | PXR24 | Depth buffers (lossy for 32-bit) |

## More Examples

### Encode Multi-layer EXR (AOVs)

```javascript
import { init, encodeExr } from 'exrs';

await init();

const bytes = encodeExr({
  width: 1920,
  height: 1080,
  layers: [
    { name: 'beauty', channelNames: ['R', 'G', 'B', 'A'], interleavedPixels: beautyData, compression: 'piz' },
    { name: 'depth', channelNames: ['Z'], interleavedPixels: depthData, compression: 'pxr24' },
    { name: 'normals', channelNames: ['R', 'G', 'B'], interleavedPixels: normalsData, compression: 'zip16' }
  ]
});
```

### WebGL Render Buffer Export

```javascript
import { init, encodeRgbaExr } from 'exrs';

await init();

const gl = canvas.getContext('webgl2');
const pixels = new Float32Array(width * height * 4);
gl.readPixels(0, 0, width, height, gl.RGBA, gl.FLOAT, pixels);

// EXR is often stored top-to-bottom, WebGL is bottom-to-top.
// You might need to flip your pixels vertically before encoding.

const bytes = encodeRgbaExr({
  width,
  height,
  interleavedRgbaPixels: pixels,
  compression: 'piz'
});
```

### Load and Display EXR

```javascript
import { init, decodeRgbaExr } from 'exrs';

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

  imageData.data[i * 4]     = Math.min(255, (r / (1 + r)) * 255);
  imageData.data[i * 4 + 1] = Math.min(255, (g / (1 + g)) * 255);
  imageData.data[i * 4 + 2] = Math.min(255, (b / (1 + b)) * 255);
  imageData.data[i * 4 + 3] = a * 255;
}

ctx.putImageData(imageData, 0, 0);
```

## License

BSD-3-Clause

## Contributing

Prerequisites:
- Rust `cargo`, `npm` and `nodejs`
- `cargo install wasm-pack`

### Building and Testing

1. `cd exrs-wasm/js`
2. `npm install`
3. `npm run test` (this also rebuilds the whole wasm package every time)
