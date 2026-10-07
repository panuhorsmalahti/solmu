import { test, expect } from '@playwright/test'

test('Memories browse newest first, manage entries, and update live without losing a draft', async ({ page, request }) => {
  const old = await (await request.post('/api/v1/memories', { data: { content: 'My cat is Miso' } })).json()
  await page.goto('/memories')
  await expect(page.getByRole('heading', { name: 'Memories', exact: true })).toBeVisible()
  await expect(page.getByText('My cat is Miso', { exact: true })).toBeVisible()
  await expect(page.locator('time').first()).toContainText('Saved')
  await page.getByLabel('Memory content').fill('This draft stays while memory changes')
  const newerResponse = await request.post('/api/v1/memories', { data: { content: 'Miso the cat likes salmon' } })
  expect(newerResponse.ok()).toBeTruthy()
  await expect(page.getByText('Miso the cat likes salmon', { exact: true })).toBeVisible()
  await expect(page.getByLabel('Memory content')).toHaveValue('This draft stays while memory changes')
  await page.getByRole('button', { name: 'Add memory' }).click()
  await expect(page.locator('.memory-card').first()).toContainText('This draft stays while memory changes')
  if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
    await page.setViewportSize({ width: 1440, height: 1000 })
    await page.screenshot({ path: 'docs/screenshots/web-memories.png' })
  }
  await page.locator('.memory-card').first().getByRole('button', { name: 'Edit' }).click()
  await page.getByLabel('Memory content').fill('Miso prefers salmon')
  await page.getByRole('button', { name: 'Save changes' }).click()
  await expect(page.getByText('Miso prefers salmon', { exact: true })).toBeVisible()
  await page.locator(`.memory-card`).filter({ hasText: 'My cat is Miso' }).getByRole('button', { name: 'Delete' }).click()
  await expect(page.getByText('My cat is Miso', { exact: true })).toHaveCount(0)
  await request.delete(`/api/v1/memories/${old.id}`)
})
