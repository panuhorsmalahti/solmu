import { defineConfig, devices } from '@playwright/test'
import { resolve } from 'node:path'

const root = process.cwd()

export default defineConfig({
  testDir: '.',
  fullyParallel: true,
  reporter: 'list',
  use: { ...devices['Desktop Chrome'], baseURL: 'http://127.0.0.1:5174', trace: 'retain-on-failure' },
  webServer: {
    command: 'npm run dev --workspace @solmu/congregator-web -- --host 127.0.0.1 --port 5174 --strictPort',
    cwd: root,
    url: 'http://127.0.0.1:5174',
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
})
