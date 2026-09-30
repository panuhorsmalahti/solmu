import { test, expect } from '@playwright/test'

test.beforeEach(async ({ request }) => {
  const response = await request.get('/api/v1/threads?limit=100')
  for (const thread of (await response.json()).items) await request.delete('/api/v1/threads/' + thread.id)
})

test('web compacts visible history and continues from the summary', async ({ page, request }) => {
  await page.goto('/')
  await expect(page.getByLabel('Message Solmu', { exact: true })).toBeEnabled()
  const thread = (await (await request.get('/api/v1/threads')).json()).items[0]
  for (const prompt of ['First web turn', 'Second web turn']) {
    await page.getByLabel('Message Solmu', { exact: true }).fill(prompt)
    await page.getByRole('button', { name: 'Send message' }).click()
    await expect(page.getByText('Hello from Solmu', { exact: true })).toBeVisible()
    await expect(page.getByLabel('Message Solmu', { exact: true })).toBeEnabled()
  }
  await page.getByRole('button', { name: 'Compact conversation' }).click()
  await expect(page.getByText(/Conversation summary \(compacted\)/)).toBeVisible()
  if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
    await page.setViewportSize({ width: 1440, height: 1000 })
    await page.screenshot({ path: 'docs/screenshots/web.png', fullPage: true })
  }
  await expect(page.getByText('First web turn', { exact: true })).toHaveCount(0)
  await expect.poll(async () => {
    const response = await request.get('/api/v1/threads/' + thread.id + '/messages')
    return (await response.json()).items.length
  }).toBe(1)
  await page.getByLabel('Message Solmu', { exact: true }).fill('Continue after compaction')
  await page.getByRole('button', { name: 'Send message' }).click()
  await expect(page.getByText('Hello from Solmu', { exact: true })).toBeVisible()
  await expect(page.getByText('Continue after compaction', { exact: true })).toBeVisible()
})
