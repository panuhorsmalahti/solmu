import { defineConfig } from '@playwright/test'
export default defineConfig({
  testDir: '.', testMatch: '*.spec.ts', workers: 1,
  use: { browserName: 'chromium' },
  webServer: { command: 'node website/serve.mjs --base /solmu/', cwd: '../..', url: 'http://127.0.0.1:4174/solmu/', reuseExistingServer: false },
})
