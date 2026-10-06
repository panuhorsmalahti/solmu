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

test('Docker setup remains available in the docs', async ({ page }) => {
  await page.goto(`${home}docs/running/`)
  await expect(page.getByRole('heading', { name: 'Docker backend' })).toBeVisible()
  await expect(page.locator('article')).toContainText('docker run --rm --name solmu')
  await expect(page.locator('article')).toContainText('Boxer over Docker')
})

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
  await expect(page.getByRole('heading', { name: 'Audit', exact: true })).toBeVisible()
  await expect(page.locator('article')).toContainText('newest first')
  await expect(page.locator('article')).toContainText('prompt cache hit rate')
  await expect(page.locator('article')).toContainText('PageUp')
  expect(await page.locator('article h3').allTextContents()).toEqual(['Terminal client', 'Desktop client', 'Web client', 'Muxer'])
  await expect(page.locator('article img[alt="Web Audit"]')).toBeVisible()
})

test('iOS guide is published with native build and screenshot details', async ({ page }) => {
  await page.goto(`${home}docs/ios/`)
  await expect(page.getByRole('heading', { name: 'iOS', exact: true })).toBeVisible()
  await expect(page.locator('article')).toContainText('native SwiftUI client')
  await expect(page.locator('article')).toContainText('XcodeGen')
  await expect(page.locator('article img[alt="Solmu native iOS client"]')).toBeVisible()
})

test('multi-client guides label each screenshot with its client', async ({ page }) => {
  for (const guide of ['audit', 'mcp', 'plugins', 'skills', 'tasks']) {
    await page.goto(`${home}docs/${guide}/`)
    const article = page.locator('article')
    for (const client of ['Terminal client', 'Desktop client', 'Web client', 'Muxer']) {
      const heading = article.getByRole('heading', { level: 3, name: client, exact: true })
      await expect(heading).toBeVisible()
      await expect(heading.locator('xpath=following-sibling::p[1]/img')).toBeVisible()
    }
  }
  await page.goto(`${home}docs/profile/`)
  for (const client of ['Terminal client', 'Desktop client', 'Web client']) {
    const heading = page.locator('article').getByRole('heading', { level: 3, name: client, exact: true })
    await expect(heading.locator('xpath=following-sibling::p[1]/img')).toBeVisible()
  }
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

test('webhook setup is published from the shared docs and linked on the website', async ({ page }) => {
  await page.goto(`${home}docs/webhooks/`)
  await expect(page.getByRole('heading', { name: 'Webhooks', exact: true })).toBeVisible()
  await expect(page.locator('article')).toContainText('X-Hub-Signature-256')
  await expect(page.locator('article')).toContainText('SOLMU_WEBHOOK_BIND_ADDR')
  await expect(page.locator('article')).toContainText('at least 16 characters')
  await expect(page.locator('article img[alt="Web webhook settings"]')).toBeVisible()
  await page.goto(home)
  await page.getByRole('link', { name: 'Explore Webhooks' }).click()
  await expect(page).toHaveURL(`${home}docs/webhooks/`)
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
  await expect(page.locator('article')).toContainText('--credential separately when the agent should receive a proxy session token')
  await expect(page.locator('article')).toContainText('boxer attach <session-id>')
  await expect(page.locator('article')).toContainText('Ctrl-] followed by d')
  await expect(page.locator('article')).toContainText('boxer ps --all')
  await expect(page.locator('article')).toContainText('boxer ps --json')
  await expect(page.locator('article')).toContainText('boxer pause <session-id>')
  await expect(page.locator('article')).toContainText('boxer resume <session-id>')
  await expect(page.locator('article')).toContainText('boxer stop <session-id> --timeout 15')
  await expect(page.locator('article')).toContainText('boxer stop <session-id> --force')
  await expect(page.locator('article')).toContainText('boxer prune --dry-run')
  await expect(page.getByRole('heading', { name: 'Run Pi, OpenCode, Claude Code, or Codex' })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Create a named policy profile' })).toBeVisible()
  await expect(page.locator('article')).toContainText('BOXER_PROFILE_DIR')
  await expect(page.locator('article pre').filter({ hasText: 'boxer --profile reviewer --cwd /path/to/project -- solmu' })).toBeVisible()
  await expect(page.locator('article pre').filter({ hasText: 'boxer --profile codex --cwd /path/to/project' })).toBeVisible()
  await expect(page.locator('article pre').filter({ hasText: 'boxer --profile claude-code --cwd /path/to/project' })).toBeVisible()
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
  const dockerCommand = page.locator('article pre').filter({ hasText: 'docker run --rm' })
  await expect(dockerCommand).toHaveCount(1)
  await expect(dockerCommand).toContainText('docker run --rm')
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
