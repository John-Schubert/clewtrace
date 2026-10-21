// @begin CT-PLT-001, CT-PLT-003
// Vite build for the desktop UI. The output in dist/ is embedded in the app binary, so the
// app loads nothing from the network at run time.
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
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
});
// @end
