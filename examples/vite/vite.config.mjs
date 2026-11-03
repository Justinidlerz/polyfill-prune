import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import polyfillPrune from '@polyfill-prune/vite-plugin';

export default defineConfig({
  root: fileURLToPath(new URL('.', import.meta.url)),
  plugins: [polyfillPrune()],
  optimizeDeps: { include: ['@polyfill-prune/example-vendor'] },
  build: {
    target: ['chrome100', 'safari15.4'],
    outDir: '../../outputs/vite',
    emptyOutDir: true,
    sourcemap: true,
  },
});
