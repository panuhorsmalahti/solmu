import { test, expect } from '@playwright/test'
import { mkdir } from 'node:fs/promises'

test.beforeEach(async ({ request }) => {
  const response = await request.get('/api/v1/threads?limit=100')
  for (const thread of (await response.json()).items) await request.delete(`/api/v1/threads/${thread.id}`)
})

test('incremental replies, generated title, saved history, rename and delete', async ({ page, request }) => {
  await page.goto('/')
  await expect(page.getByLabel('Message Solmu', { exact: true })).toBeEnabled()
  const threads = await (await request.get('/api/v1/threads')).json()
  expect(threads.items).toHaveLength(1)
  const id = threads.items[0].id
  await expect(page).toHaveURL(new RegExp(`/threads/${id}$`))
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
  await page.reload()
  await expect(page.getByText('Hello from Solmu', { exact: true })).toBeVisible()
  expect((await (await request.get('/api/v1/threads')).json()).items).toHaveLength(1)
  if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
    await mkdir('docs/screenshots', { recursive: true })
    await page.setViewportSize({ width: 1440, height: 1000 })
    await page.screenshot({ path: 'docs/screenshots/web.png' })
  }
  await page.getByRole('button', { name: 'Delete thread' }).click()
  await expect(page.getByLabel('Thread selector').getByText('A thoughtful project')).toHaveCount(0)
  expect((await request.get(`/api/v1/threads/${id}`)).status()).toBe(404)
})
