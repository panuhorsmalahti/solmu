import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: '.', testMatch: '**/*.spec.ts', fullyParallel: false, workers: 1,
  timeout: 30_000, expect: { timeout: 10_000 },
  reporter: process.env.CI ? 'github' : 'list',
  use: { ...devices['Desktop Chrome'], baseURL: 'http://127.0.0.1:5175', trace: 'retain-on-failure', screenshot: 'only-on-failure' },
  webServer: [
    { command: 'node e2e/web/server.mjs', cwd: '..', url: 'http://127.0.0.1:5175', reuseExistingServer: false, timeout: 60_000 },
    { command: 'node website/serve.mjs', cwd: '..', url: 'http://127.0.0.1:4174', reuseExistingServer: false, timeout: 20_000 },
  ],
})
