import { test, expect } from '@playwright/test'


test.beforeEach(async ({ request }) => {
  const response = await request.get('/api/v1/threads?limit=100')
  for (const thread of (await response.json()).items) await request.delete(`/api/v1/threads/${thread.id}`)
})

test('provider failure keeps user message; Enter, suggestions, live updates, and mobile layout', async ({ page, request }) => {
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
  await expect(page.getByRole('button', { name: 'Refresh conversations' })).toHaveCount(0)
  await expect(page.getByLabel('Conversation title')).toHaveValue('Updated elsewhere')
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true)
})

test('Stop cancels the backend reply and preserves the user message', async ({ page, request }) => {
  await page.goto('/')
  await expect(page.getByLabel('Message Solmu', { exact: true })).toBeEnabled()
  await page.getByLabel('Message Solmu', { exact: true }).fill('A reply I will stop')
  await page.getByRole('button', { name: 'Send message' }).click()
  await expect(page.getByLabel('Streaming reply')).toContainText('Hello')
  await page.getByRole('button', { name: 'Stop response' }).click()
  await expect(page.getByLabel('Message Solmu', { exact: true })).toBeEnabled()
  await expect(page.getByLabel('Streaming reply')).toHaveCount(0)
  const id = page.url().split('/').pop()
  expect((await (await request.get(`/api/v1/threads/${id}/messages`)).json()).items).toHaveLength(1)
  await page.getByLabel('Message Solmu', { exact: true }).fill('Continue after stopping')
  await page.getByRole('button', { name: 'Send message' }).click()
  await expect(page.getByText('Hello from Solmu', { exact: true })).toBeVisible()
})
