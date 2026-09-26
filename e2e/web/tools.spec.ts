import { test, expect } from '@playwright/test'

test.beforeEach(async ({ request }) => {
  const threads = await (await request.get('/api/v1/threads?limit=100')).json()
  for (const thread of threads.items) await request.delete(`/api/v1/threads/${thread.id}`)
})

test('tool activity streams, expands, survives navigation and can be stopped', async ({ page, request }) => {
  await page.goto('/')
  const input = page.getByLabel('Message Solmu', { exact: true })
  await expect(input).toBeEnabled()
  const threads = await (await request.get('/api/v1/threads')).json()
  await expect(page.getByLabel('Workspace', { exact: true })).toHaveText(threads.items[0].workspace)
  await input.fill('Inspect this workspace')
  await page.getByRole('button', { name: 'Send message' }).click()
  await expect(page.locator('summary', { hasText: 'Bash · completed' })).toBeVisible()
  await expect(page.getByText('Hello from Solmu', { exact: true })).toBeVisible()
  await expect(input).toBeEnabled()
  await page.locator('summary', { hasText: 'Read · completed' }).click()
  await expect(page.locator('details[open]')).toContainText('Hello tools')
  const url = page.url()
  if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
    await page.setViewportSize({ width: 1440, height: 1000 })
    await page.screenshot({ path: 'docs/screenshots/web.png' })
  }
  await page.reload()
  await expect(page.locator('summary', { hasText: 'Bash · completed' })).toBeVisible()
  await expect(page).toHaveURL(url)
  await expect(input).toBeEnabled()
  await input.fill('Run a long task')
  await page.getByRole('button', { name: 'Send message' }).click()
  await expect(page.locator('summary', { hasText: 'Bash · running' })).toBeVisible()
  await page.getByRole('button', { name: 'Stop response' }).click()
  await expect(input).toBeEnabled()
  await expect(page.locator('summary', { hasText: 'Bash · cancelled' })).toBeVisible()
})
