import { test, expect } from '@playwright/test'
import { mkdir } from 'node:fs/promises'

test.beforeEach(async ({ request }) => {
  const response = await request.get('/api/v1/threads?limit=100')
  for (const thread of (await response.json()).items) await request.delete(`/api/v1/threads/${thread.id}`)
})

test('sidebar threads, incremental replies, generated title, history, rename and delete', async ({ page, request }) => {
  await page.goto('/')
  await expect(page.getByLabel('Message Solmu', { exact: true })).toBeEnabled()
  const threads = await (await request.get('/api/v1/threads')).json()
  expect(threads.items).toHaveLength(1)
  const id = threads.items[0].id
  await page.getByLabel('Message Solmu', { exact: true }).fill('Help me plan a thoughtful project.\nLet’s start with an idea.')
  await page.getByRole('button', { name: 'Send message' }).click()
  await expect(page.getByLabel('Streaming reply')).toContainText('Hello')
  expect((await (await request.get(`/api/v1/threads/${id}/messages`)).json()).items).toHaveLength(1)
  await expect(page.getByText('Hello from Solmu', { exact: true })).toBeVisible()
  await expect(page.getByLabel('Conversation title')).toHaveValue('A new idea')
  await expect(page.getByLabel('Thread selector').getByRole('button', { name: 'A new idea' })).toHaveAttribute('aria-current', 'true')
  await page.getByLabel('Conversation title').fill('A thoughtful project')
  await page.getByRole('button', { name: 'Rename thread' }).click()
  await expect(page.getByLabel('Thread selector').getByText('A thoughtful project')).toBeVisible()
  if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
    await mkdir('docs/screenshots', { recursive: true })
    await page.screenshot({ path: 'docs/screenshots/web.png' })
  }
  await page.getByRole('button', { name: 'New thread', exact: true }).click()
  await expect(page.getByText('A little space for your')).toBeVisible()
  await expect(page.getByLabel('Message Solmu', { exact: true })).toBeEnabled()
  await page.getByLabel('Thread selector').getByRole('button', { name: 'A thoughtful project' }).click()
  await expect(page.getByText('Hello from Solmu', { exact: true })).toBeVisible()
  await page.getByRole('button', { name: 'Delete thread' }).click()
  await expect(page.getByLabel('Thread selector').getByText('A thoughtful project')).toHaveCount(0)
  expect((await request.get(`/api/v1/threads/${id}`)).status()).toBe(404)
  await page.getByLabel('Thread selector').getByRole('button', { name: 'New conversation' }).click()
  await expect(page.getByLabel('Message Solmu', { exact: true })).toBeEnabled()
})

test('provider failure keeps user message; Enter, suggestions, refresh, and mobile layout', async ({ page, request }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto('/')
  await expect(page.getByRole('button', { name: 'Explore an idea' })).toBeEnabled()
  await page.getByRole('button', { name: 'Explore an idea' }).click()
  await expect(page.getByLabel('Message Solmu', { exact: true })).toHaveValue('Explore an idea')
  await page.getByLabel('Message Solmu', { exact: true }).fill('FAIL')
  await page.getByLabel('Message Solmu', { exact: true }).press('Enter')
  await expect(page.getByRole('alert')).toContainText('LLM request failed')
  await expect(page.getByText('FAIL', { exact: true })).toBeVisible()
  const thread = (await (await request.get('/api/v1/threads')).json()).items[0]
  expect((await (await request.get(`/api/v1/threads/${thread.id}/messages`)).json()).items).toHaveLength(1)
  await request.patch(`/api/v1/threads/${thread.id}`, { data: { title: 'Updated elsewhere' } })
  await page.getByRole('button', { name: 'Refresh conversations' }).click()
  await expect(page.getByLabel('Conversation title')).toHaveValue('Updated elsewhere')
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true)
})
