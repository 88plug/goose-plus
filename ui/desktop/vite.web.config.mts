import { defineConfig } from 'vite';
import { resolve } from 'node:path';
import tailwindcss from '@tailwindcss/vite';

// Web build: serves the same renderer in a plain browser via the web shim.
// https://vitejs.dev/config
export default defineConfig({
  base: './',

  define: {
    'process.env.GOOSE_TUNNEL': JSON.stringify(
      process.env.GOOSE_TUNNEL !== 'no' && process.env.GOOSE_TUNNEL !== 'none'
    ),
  },

  plugins: [tailwindcss()],

  publicDir: 'web-public',

  // Vite caches a copy of @aaif/goose-sdk and doesn't notice when we rebuild it
  // locally, so it serves stale code until you clear node_modules/.vite by hand.
  // Excluding it makes Vite always read the latest ui/sdk/dist build.
  optimizeDeps: {
    exclude: ['@aaif/goose-sdk'],
  },

  build: {
    target: 'esnext',
    outDir: 'dist-web',
    emptyOutDir: true,
    rollupOptions: {
      input: resolve(__dirname, 'web.html'),
    },
  },
});
