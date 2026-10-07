import { test, expect } from '@playwright/test'

async function askSolmuToRemember(request: import('@playwright/test').APIRequestContext, content: string) {
  const thread = await (await request.post('/api/v1/threads', { data: {} })).json()
  const message = await (await request.post(`/api/v1/threads/${thread.id}/messages`, {
    data: { content: `TOOLS ${JSON.stringify([{ name: 'Memory', arguments: { action: 'write', content } }])}` },
  })).json()
  const response = await request.post(`/api/v1/threads/${thread.id}/responses`, { data: { message_id: message.id } })
  expect(response.ok()).toBeTruthy()
  await response.text()
  return thread.id as string
}

test('Memories are read-only, newest first, and update live', async ({ page, request }) => {
  const firstThread = await askSolmuToRemember(request, 'My cat is Miso')
  await page.goto('/memories')
  await expect(page.getByRole('heading', { name: 'Memories', exact: true })).toBeVisible()
  await expect(page.getByText('My cat is Miso', { exact: true })).toBeVisible()
  await expect(page.locator('time').first()).toContainText('Saved')
  expect(await page.getByRole('button', { name: /Add memory|Save changes|Edit|Delete/ }).count()).toBe(0)
  expect(await page.locator('textarea').count()).toBe(0)
  const secondThread = await askSolmuToRemember(request, 'Miso the cat likes salmon')
  await expect(page.getByText('Miso the cat likes salmon', { exact: true })).toBeVisible()
  await expect(page.locator('.memory-card').first()).toContainText('Miso the cat likes salmon')
  await request.delete(`/api/v1/threads/${secondThread}`)
  await request.delete(`/api/v1/threads/${firstThread}`)
  await expect(page.getByText('A new idea', { exact: true })).toHaveCount(0)
  if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
    await page.setViewportSize({ width: 1440, height: 1000 })
    await page.screenshot({ path: 'docs/screenshots/web-memories.png' })
  }
})
