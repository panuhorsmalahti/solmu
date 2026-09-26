import { chromium } from '@playwright/test'
import { mkdir } from 'node:fs/promises'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

// Render terminal cells recorded by the real CLI e2e test.
const browser = await chromium.launch()
try {
  const page = await browser.newPage({ viewport: { width: 1000, height: 688 }, deviceScaleFactor: 1 })
  await page.goto(pathToFileURL(resolve('artifacts/cli.html')).href)
  await page.addStyleTag({ content: 'span { vertical-align: top }' })
  await mkdir('docs/screenshots', { recursive: true })
  await page.screenshot({ path: 'docs/screenshots/cli.png' })
} finally {
  await browser.close()
}
