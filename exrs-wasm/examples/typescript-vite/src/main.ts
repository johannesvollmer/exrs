import {
  init,
  encodeRgbaExr,
  decodeRgbaExr,
  encodeExr,
  decodeExr,
  RGBA,
  RGB,
} from 'exrs';

// Helper to download a Uint8Array as a file
function downloadExr(data: Uint8Array, filename: string) {
  const blob = new Blob([data as BlobPart], { type: 'image/x-exr' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

// Helper to display Float32 pixels on a canvas (assumes linear RGB, applies gamma)
function displayOnCanvas(
  canvas: HTMLCanvasElement,
  pixels: Float32Array,
  width: number,
  height: number,
  channels: number
) {
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d')!;
  const imageData = ctx.createImageData(width, height);

  for (let i = 0; i < width * height; i++) {
    // Apply simple gamma correction for display (linear to sRGB)
    const r = Math.pow(Math.max(0, Math.min(1, pixels[i * channels + 0])), 1 / 2.2);
    const g = Math.pow(Math.max(0, Math.min(1, pixels[i * channels + 1])), 1 / 2.2);
    const b = Math.pow(Math.max(0, Math.min(1, pixels[i * channels + 2])), 1 / 2.2);
    const a = channels === 4 ? pixels[i * channels + 3] : 1;

    imageData.data[i * 4 + 0] = r * 255;
    imageData.data[i * 4 + 1] = g * 255;
    imageData.data[i * 4 + 2] = b * 255;
    imageData.data[i * 4 + 3] = a * 255;
  }

  ctx.putImageData(imageData, 0, 0);
}

// Example 1: Encode and decode an RGBA image
async function exampleRgba() {
  const output = document.getElementById('output-rgba')!;
  const canvas = document.getElementById('canvas-rgba') as HTMLCanvasElement;

  const width = 256;
  const height = 256;
  const pixels = new Float32Array(width * height * 4);

  // Create a gradient with some transparency
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      pixels[i + 0] = x / width;           // R: horizontal gradient
      pixels[i + 1] = y / height;          // G: vertical gradient
      pixels[i + 2] = 1 - (x / width);     // B: inverse horizontal
      pixels[i + 3] = 1.0;                 // A: fully opaque
    }
  }

  // Encode to EXR
  const exrBytes = encodeRgbaExr({
    width,
    height,
    interleavedRgbaPixels: pixels,
    precision: 'f16',      // Use 16-bit float for smaller file
    compression: 'piz',    // Good compression for noisy data
  });

  output.textContent = `Original: ${width}x${height} RGBA (${pixels.byteLength} bytes)\n`;
  output.textContent += `Encoded EXR: ${exrBytes.byteLength} bytes\n`;

  // Decode the EXR back
  const decoded = decodeRgbaExr(exrBytes);
  output.textContent += `Decoded: ${decoded.width}x${decoded.height}\n`;

  // Display on canvas
  displayOnCanvas(canvas, decoded.interleavedRgbaPixels, decoded.width, decoded.height, 4);

  // Setup download button
  const downloadBtn = document.getElementById('download-rgba') as HTMLButtonElement;
  downloadBtn.disabled = false;
  downloadBtn.onclick = () => downloadExr(exrBytes, 'gradient.exr');
}

// Example 2: Multi-layer EXR (AOV export)
async function exampleMultiLayer() {
  const output = document.getElementById('output-multilayer')!;

  const width = 64;
  const height = 64;

  // Create beauty pass (RGBA)
  const beauty = new Float32Array(width * height * 4);
  for (let i = 0; i < width * height; i++) {
    beauty[i * 4 + 0] = Math.random() * 0.8 + 0.2; // R
    beauty[i * 4 + 1] = Math.random() * 0.6 + 0.1; // G
    beauty[i * 4 + 2] = Math.random() * 0.4;       // B
    beauty[i * 4 + 3] = 1.0;                        // A
  }

  // Create depth pass (single channel Z)
  const depth = new Float32Array(width * height);
  for (let i = 0; i < width * height; i++) {
    const x = (i % width) / width;
    const y = Math.floor(i / width) / height;
    depth[i] = Math.sqrt(x * x + y * y) * 10; // Distance from corner
  }

  // Create normals pass (RGB)
  const normals = new Float32Array(width * height * 3);
  for (let i = 0; i < width * height; i++) {
    normals[i * 3 + 0] = 0.5; // Nx
    normals[i * 3 + 1] = 0.5; // Ny
    normals[i * 3 + 2] = 1.0; // Nz (facing camera)
  }

  // Encode multi-layer EXR
  const exrBytes = encodeExr({
    width,
    height,
    layers: [
      {
        name: 'beauty',
        channelNames: RGBA,
        interleavedPixels: beauty,
        precision: 'f16',
        compression: 'piz',
      },
      {
        name: 'depth',
        channelNames: ['Z'],
        interleavedPixels: depth,
        precision: 'f32',      // Full precision for depth
        compression: 'pxr24',  // Good for depth data
      },
      {
        name: 'normals',
        channelNames: RGB,
        interleavedPixels: normals,
        precision: 'f16',
        compression: 'zip16',
      },
    ],
  });

  output.textContent = `Encoded multi-layer EXR: ${exrBytes.byteLength} bytes\n`;
  output.textContent += `Contains 3 layers: beauty (RGBA), depth (Z), normals (RGB)\n\n`;

  // Decode and inspect
  const decoded = decodeExr(exrBytes);
  output.textContent += `Decoded ${decoded.layers.length} layers:\n`;

  for (const layer of decoded.layers) {
    output.textContent += `\n  Layer: "${layer.name}"\n`;
    output.textContent += `    Channels: ${layer.channelNamesAlphabetical.join(', ')}\n`;
    output.textContent += `    Has RGBA: ${layer.containsChannelNames(RGBA)}\n`;
    output.textContent += `    Has RGB: ${layer.containsChannelNames(RGB)}\n`;
    output.textContent += `    Has Z: ${layer.containsChannelNames(['Z'])}\n`;

    // Extract specific channels
    if (layer.containsChannelNames(['Z'])) {
      const zData = layer.getInterleavedPixels(['Z']);
      if (zData) {
        output.textContent += `    Z range: ${Math.min(...zData).toFixed(2)} - ${Math.max(...zData).toFixed(2)}\n`;
      }
    }
  }

  // Setup download button
  const downloadBtn = document.getElementById('download-multilayer') as HTMLButtonElement;
  downloadBtn.disabled = false;
  downloadBtn.onclick = () => downloadExr(exrBytes, 'multilayer.exr');
}

// Example 3: File upload and decode
function setupFileUpload() {
  const input = document.getElementById('file-input') as HTMLInputElement;
  const canvas = document.getElementById('canvas-upload') as HTMLCanvasElement;
  const output = document.getElementById('output-upload')!;

  input.addEventListener('change', async () => {
    const file = input.files?.[0];
    if (!file) return;

    const buffer = await file.arrayBuffer();
    const data = new Uint8Array(buffer);

    try {
      const decoded = decodeExr(data);
      output.textContent = `File: ${file.name}\n`;
      output.textContent += `Size: ${decoded.width}x${decoded.height}\n`;
      output.textContent += `Layers: ${decoded.layers.length}\n\n`;

      for (const layer of decoded.layers) {
        output.textContent += `Layer: "${layer.name ?? '(unnamed)'}"\n`;
        output.textContent += `  Channels: ${layer.channelNamesAlphabetical.join(', ')}\n`;
      }

      // Display first layer with RGB(A) data
      const displayLayer = decoded.layers.find(
        (l) => l.containsChannelNames(RGBA) || l.containsChannelNames(RGB)
      );

      if (displayLayer) {
        const hasAlpha = displayLayer.containsChannelNames(RGBA);
        const channels = hasAlpha ? RGBA : RGB;
        const pixels = displayLayer.getInterleavedPixels(channels);
        if (pixels) {
          displayOnCanvas(
            canvas,
            pixels,
            decoded.width,
            decoded.height,
            channels.length
          );
        }
      }
    } catch (e) {
      output.textContent = `Error decoding: ${e}`;
    }
  });
}

// Initialize and run examples
async function main() {
  try {
    // Must initialize WASM before using any functions
    await init();
    console.log('EXRS WASM initialized');

    await exampleRgba();
    await exampleMultiLayer();
    setupFileUpload();
  } catch (e) {
    console.error('Error:', e);
    document.body.innerHTML = `<h1>Error</h1><pre>${e}</pre>`;
  }
}

main();
