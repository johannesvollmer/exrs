import { defineConfig } from 'vite';

export default defineConfig({
  resolve: {
    preserveSymlinks: true,
  },
  optimizeDeps: {
    // Exclude WASM packages from pre-bundling so import.meta.url resolves correctly
    exclude: ['exrs-raw-wasm-bindgen'],
    // But include shallow-equals since it's CJS and needs conversion
    include: ['exrs > shallow-equals'],
  },
  server: {
    fs: {
      // Allow serving files from the pkg directory (where WASM lives)
      allow: ['.', '../../js', '../../pkg'],
    },
  },
  build: {
    target: 'esnext',
  },
});
