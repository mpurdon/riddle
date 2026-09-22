import { defineConfig } from '@playwright/test'

// Headless UI verification. Two modes:
//   default        the React client in mock mode (no backend)
//   RIDDLE_BRIDGE  set to the dev bridge's bridge.json path: the client talks
//                  to the real Rust core through the loopback bridge
export default defineConfig({
  testDir: './e2e',
  timeout: 60_000,
  retries: 0,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:1430',
    headless: true,
    viewport: { width: 1440, height: 900 },
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'pnpm dev',
    url: 'http://127.0.0.1:1430',
    reuseExistingServer: true,
    timeout: 60_000,
  },
})
