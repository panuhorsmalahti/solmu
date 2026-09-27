import { defineConfig } from '@playwright/test'

export default defineConfig({ testDir: '..', testMatch: ['**/installers.spec.ts', '**/install.spec.ts', '**/*.install.spec.ts'], workers: 1, timeout: 30_000 })
