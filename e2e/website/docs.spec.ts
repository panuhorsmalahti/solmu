import { test, expect } from '@playwright/test'
import { readFile, mkdir, writeFile, rm } from 'node:fs/promises'
import { randomUUID } from 'node:crypto'
import { join, resolve } from 'node:path'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'

const repository = resolve(__dirname, '../..')
const execute = promisify(execFile)
const buildSite = () => execute(process.execPath, [join(repository, 'website/build.mjs')], { cwd: repository })

const home = 'http://127.0.0.1:4174/solmu/'

test('MCP guide is published from the root docs and shows live status in every client', async ({ page }) => {
  await page.goto(`${home}docs/mcp/`)
  await expect(page.getByRole('heading', { name: 'MCP tools' })).toBeVisible()
  await expect(page.locator('article')).toContainText('.mcp.json')
  await expect(page.locator('article')).toContainText('Streamable HTTP')
  await expect(page.locator('article')).toContainText('/mcp')
  await expect(page.locator('article img[alt="Desktop MCP status"]')).toBeVisible()
})

test('plugins guide is published from root docs', async ({ page }) => {
  await page.goto(`${home}docs/plugins/`)
  await expect(page.getByRole('heading', { name: 'Agent Plugins' })).toBeVisible()
  await expect(page.locator('article')).toContainText('.agents/plugins/')
  await expect(page.locator('article')).toContainText('/plugins')
  await expect(page.locator('article img[alt="Desktop plugins"]')).toBeVisible()
})

test('Audit guide is published and links client screenshots', async ({ page }) => {
  await page.goto(`${home}docs/audit/`)
  await expect(page.getByRole('heading', { name: 'Audit' })).toBeVisible()
  await expect(page.locator('article')).toContainText('newest first')
  await expect(page.locator('article')).toContainText('prompt cache hit rate')
  await expect(page.locator('article')).toContainText('PageUp')
  await expect(page.locator('article img[alt="Web Audit"]')).toBeVisible()
})

test('Scheduled tasks are introduced on the website and documented from root docs', async ({ page }) => {
  await page.goto(home)
  await expect(page.getByRole('link', { name: 'Explore Tasks' })).toBeVisible()
  await page.getByRole('link', { name: 'Explore Tasks' }).click()
  await expect(page).toHaveURL(`${home}docs/tasks/`)
  await expect(page.getByRole('heading', { name: 'Scheduled tasks' })).toBeVisible()
  await expect(page.locator('article')).toContainText('/task cron')
  await expect(page.locator('article img[alt="Tasks on the web"]')).toBeVisible()
})

test('Boxer guide uses default backend settings and describes explicit Linux network routes', async ({ page }) => {
  await page.goto(`${home}docs/boxer/`)
  const example = page.locator('article pre').filter({ hasText: 'boxer --profile solmu --cwd /path/to/project -- solmu-backend' })
  await expect(example).toBeVisible()
  await expect(example).not.toContainText('SOLMU_BIND_ADDR=')
  await expect(example).not.toContainText('SOLMU_BACKEND_URL=')
  await expect(example).toContainText('solmu')
  await expect(page.locator('article')).toContainText('--allow-host api.openai.com --publish 3000')
  await expect(page.locator('article')).toContainText('--allow-local 127.0.0.1:3000')
  await expect(page.locator('article')).toContainText('Network routing does not yet provide credential injection')
})

test('Docs is linked from the website and every Markdown guide is published', async ({ page }) => {
  await page.goto(home)
  await page.getByRole('link', { name: 'Docs', exact: true }).click()
  await expect(page).toHaveURL(`${home}docs/`)
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Solmu documentation')
  const guides = page.getByRole('navigation', { name: 'Documentation guides' })
  const paths = await guides.getByRole('link').evaluateAll((links: HTMLAnchorElement[]) => links.map((link) => link.href))
  expect(paths.length).toBeGreaterThan(20)
  for (const path of paths) {
    expect(path).toMatch(/^http:\/\/127\.0\.0\.1:4174\/solmu\/docs\//)
    const response = await page.request.get(path)
    expect(response.status(), path).toBe(200)
    expect(await response.text()).toContain('<article>')
  }
  await guides.getByRole('link', { name: 'Terminal client', exact: true }).click()
  await expect(page).toHaveURL(`${home}docs/cli/`)
  await expect(page).toHaveTitle('Terminal client · Solmu docs')
  await expect(guides.getByRole('link', { name: 'Terminal client', exact: true })).toHaveAttribute('aria-current', 'page')
  const source = await readFile(join(repository, 'docs/cli.md'), 'utf8')
  expect(source).toContain('# Terminal client')
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Terminal client')
  await expect(page.locator('article')).toContainText('solmu --thread')
  const image = page.locator('article img[alt="Solmu client screenshot"]')
  await expect(image).toHaveAttribute('src', '../../screenshots/cli.png')
  await expect.poll(() => image.evaluate((element: HTMLImageElement) => element.complete && element.naturalWidth > 0)).toBe(true)
  await expect(page.getByRole('link', { name: 'View this page on GitHub' })).toHaveAttribute('href', 'https://github.com/panuhorsmalahti/solmu/blob/main/docs/cli.md')
})

test('guide links, section anchors, tables, and installation commands work under the Pages project path', async ({ page }) => {
  await page.goto(`${home}docs/`)
  await page.locator('article').getByRole('link', { name: 'choose individual modules' }).click()
  await expect(page).toHaveURL(`${home}docs/releases/#individual-components`)
  await expect(page.getByRole('heading', { name: 'Individual components', exact: true })).toBeInViewport()
  await expect(page.locator('article table').first()).toBeVisible()
  await expect(page.locator('article pre').first()).toContainText('docker run --rm')
  await page.getByRole('navigation', { name: 'On this page' }).getByRole('link', { name: 'Docker images' }).click()
  await expect(page).toHaveURL(/#docker-images$/)
  await expect(page.getByRole('heading', { name: 'Docker images', exact: true })).toBeInViewport()
  await expect(page.locator('article').getByRole('link', { name: 'Web', exact: true })).toHaveAttribute('href', 'https://github.com/panuhorsmalahti/solmu/blob/main/clients/web/README.md#install')
  await page.getByRole('navigation', { name: 'Documentation guides' }).getByRole('link', { name: 'Install the complete Solmu bundle' }).click()
  await expect(page.locator('article pre').first()).toContainText('curl -fsSL')
})

test('guide filter reacts immediately and can recover from an empty result', async ({ page }) => {
  await page.goto(`${home}docs/`)
  const guides = page.getByRole('navigation', { name: 'Documentation guides' })
  await page.getByRole('searchbox', { name: 'Find a guide' }).fill('terminal')
  await expect(guides.getByRole('link', { name: 'Terminal client', exact: true })).toBeVisible()
  await expect(guides.getByRole('link', { name: 'Desktop client', exact: true })).toBeHidden()
  await page.getByRole('searchbox', { name: 'Find a guide' }).fill('no-such-guide')
  await expect(page.getByRole('status')).toHaveText('No matching guides.')
  await expect(guides.getByRole('link')).toHaveCount(0)
  await page.getByRole('searchbox', { name: 'Find a guide' }).fill('')
  await expect(guides.getByRole('link', { name: 'Desktop client', exact: true })).toBeVisible()
  await expect(page.getByRole('status')).toBeHidden()
})

test('documentation works on mobile and without JavaScript, with keyboard navigation', async ({ page, browser }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto(home)
  await expect(page.getByRole('link', { name: 'Docs', exact: true })).toBeVisible()
  await page.getByRole('link', { name: 'Docs', exact: true }).click()
  const menu = page.locator('.guide-menu')
  await expect(menu).not.toHaveAttribute('open', '')
  await page.getByText('Browse guides', { exact: true }).click()
  await menu.getByRole('link', { name: 'Conversation API', exact: true }).click()
  await expect(page).toHaveURL(`${home}docs/api/`)
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
  await page.keyboard.press('Tab')
  await expect(page.getByRole('link', { name: 'Skip to content' })).toBeFocused()
  await page.keyboard.press('Enter')
  await expect(page).toHaveURL(/#main$/)
  const noScript = await browser.newContext({ javaScriptEnabled: false })
  try {
    const plain = await noScript.newPage()
    await plain.goto(`${home}docs/profile/`)
    await expect(plain.getByRole('heading', { level: 1 })).toBeVisible()
    await plain.getByRole('navigation', { name: 'Documentation guides' }).getByRole('link', { name: 'Web client', exact: true }).click()
    await expect(plain).toHaveURL(`${home}docs/web/`)
    await expect(plain.locator('article img[alt="Solmu client screenshot"]')).toBeVisible()
  } finally { await noScript.close() }
})

test('new nested Markdown files and subsequent edits publish from the source without copied guide content', async ({ page }) => {
  const name = `website-e2e-${randomUUID()}`
  const directory = join(repository, 'docs', name)
  const source = join(directory, 'guide.md')
  await mkdir(directory)
  try {
    await writeFile(source, '# A new workspace guide\n\nFresh source content.\n\n[Profile](../profile.md)\n\n## Repeat\n\nFirst section.\n\n## Repeat\n\nSecond section.\n\n<script>window.injected = true</script>\n\n[Unsafe](javascript:alert(1))\n')
    await buildSite()
    await page.goto(`${home}docs/`)
    await page.getByRole('navigation', { name: 'Documentation guides' }).getByRole('link', { name: 'A new workspace guide' }).click()
    await expect(page).toHaveURL(`${home}docs/${name}/guide/`)
    await expect(page.locator('article')).toContainText('Fresh source content.')
    await expect(page.locator('article script')).toHaveCount(0)
    await expect(page.locator('article a[href^="javascript:"]')).toHaveCount(0)
    expect(await page.evaluate(() => (window as Window & { injected?: boolean }).injected)).toBeUndefined()
    await page.getByRole('navigation', { name: 'On this page' }).getByRole('link', { name: 'Repeat', exact: true }).nth(1).click()
    await expect(page).toHaveURL(/#repeat-1$/)
    await page.locator('article').getByRole('link', { name: 'Profile', exact: true }).click()
    await expect(page).toHaveURL(`${home}docs/profile/`)
    await writeFile(source, '# A new workspace guide\n\nUpdated source content.\n')
    await buildSite()
    await page.goto(`${home}docs/${name}/guide/`)
    await expect(page.locator('article')).toContainText('Updated source content.')
    await expect(page.locator('article')).not.toContainText('Fresh source content.')
  } finally {
    await rm(directory, { recursive: true, force: true })
    await buildSite()
  }
  expect((await page.request.get(`${home}docs/${name}/guide/`)).status()).toBe(404)
})

test('documentation overview presents the website theme', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 })
  await page.goto(`${home}docs/`)
  await expect(page.getByRole('heading', { name: 'Solmu documentation', exact: true })).toBeVisible()
  if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
    await page.screenshot({ path: join(repository, 'docs/screenshots/website-docs.png') })
  }
})
