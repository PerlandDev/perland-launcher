/// <reference types="vitest/config" />
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  build: { target: 'es2022', sourcemap: true },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    // themes.test.ts reads app.css as raw text; every other stylesheet stays out of tests.
    css: { include: [/styles\/app\.css/] },
    restoreMocks: true,
    // Event times read in the computer's zone: every test runs in UTC, whatever TZ the shell exports.
    env: { TZ: 'UTC' },
  },
});
