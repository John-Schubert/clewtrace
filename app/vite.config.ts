// @begin CT-PLT-001, CT-PLT-003, CT-USE-004
// Vite build for the desktop UI. The output in dist/ is embedded in the app binary, so the
// app loads nothing from the network at run time. The test block is for vitest (UI tests).
import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig(({ mode }) => ({
  plugins: [svelte()],
  // Vitest loads Svelte's browser build, so components can mount in happy-dom.
  resolve: mode === 'test' ? { conditions: ['browser'] } : {},
  test: { environment: 'happy-dom', include: ['src/**/*.test.ts'] },
  // Tauri's dev server URL in tauri.conf.json expects this fixed port.
  clearScreen: false,
  server: { port: 1420, strictPort: true, host: 'localhost' },
  build: {
    // The oldest web views Tauri 2 supports: WebView2 on Windows, WebKit on macOS and Linux.
    target: ['chrome105', 'safari15'],
    // No data: URIs, so the content security policy can stay at 'self'.
    assetsInlineLimit: 0,
    modulePreload: { polyfill: false },
  },
}));
// @end
