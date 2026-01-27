# EXRS Example

A TypeScript example demonstrating the `exrs` WASM package for encoding and decoding OpenEXR files in the browser.

## Setup

1. First, build the main exrs package:

```bash
cd ../../js
npm install
npm run build
```

2. Install this example's dependencies:

```bash
npm install
```

## Running

Start the dev server:

```bash
npm run dev
```

Open the URL shown (typically http://localhost:5173) in your browser.

## Examples Included

1. **RGBA Encode/Decode** - Creates a gradient image, encodes to EXR with PIZ compression, decodes and displays
2. **Multi-Layer AOV** - Creates an EXR with beauty, depth, and normals layers
3. **File Upload** - Upload and decode any EXR file to inspect its contents

## Using npm link (alternative setup)

Instead of `file:../../js`, you can use npm link:

```bash
# In the js directory
cd ../../js
npm link

# In this directory
npm link exrs
```
