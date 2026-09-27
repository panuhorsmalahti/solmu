import { chromium } from '@playwright/test'
import { mkdir } from 'node:fs/promises'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

// Render terminal cells recorded by the real CLI or muxer e2e test.
const client = process.argv[2] ?? 'cli'
if (!['cli', 'cli-profile', 'muxer', 'muxer-settings', 'muxer-automation', 'muxer-terminal', 'muxer-commands'].includes(client)) throw new Error('Choose cli, cli-profile, muxer, muxer-settings, muxer-automation, muxer-terminal or muxer-commands')
const browser = await chromium.launch()
try {
  const page = await browser.newPage({ viewport: { width: client.startsWith('muxer') ? 1700 : 1000, height: client.startsWith('muxer') ? 848 : 688 }, deviceScaleFactor: 1 })
  await page.goto(pathToFileURL(resolve(`artifacts/${client}.html`)).href)
  await page.addStyleTag({ content: 'span { vertical-align: top }' })
  await mkdir('docs/screenshots', { recursive: true })
  await page.screenshot({ path: `docs/screenshots/${client}.png` })
} finally {
  await browser.close()
}
