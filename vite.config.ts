import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// Tauri expects a fixed port and watches ignore src-tauri/ to avoid recursive rebuilds.
// See https://v2.tauri.app/start/frontend/sveltekit/
export default defineConfig(async () => ({
  plugins: [sveltekit()],

  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: '127.0.0.1',
    watch: {
      ignored: ['**/src-tauri/**']
    }
  },

  envPrefix: ['VITE_', 'TAURI_ENV_*']
}));
