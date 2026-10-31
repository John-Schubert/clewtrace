// @begin CT-USE-002, CT-USE-005, CT-EFF-005
// Vitest setup for UI tests: a fake DOM from happy-dom, no browser and no network.
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [svelte()],
  resolve: { conditions: ['browser'] },
  cacheDir: 'node_modules/.vite',
  test: { environment: 'happy-dom', include: ['src/**/*.test.ts'] },
});
// @end
