import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'

// Tauri serves this on 1430 in dev; the port is fixed so tauri.conf.json's
// devUrl and the dev bridge agree.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  // The shared Mesh grammar corpus lives with the Rust parser; the mock's
  // test reads it from there (src/lib/mocks/dsl.grammar.test.ts).
  server: { port: 1430, strictPort: true, host: '127.0.0.1', fs: { allow: ['.', '../../crates/riddle-mesh/tests'] } },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: { target: 'safari16', sourcemap: true },
  test: { environment: 'jsdom', globals: false, include: ['src/**/*.test.{ts,tsx}'], setupFiles: ['src/vitest.setup.ts'], testTimeout: 20_000 },
})
