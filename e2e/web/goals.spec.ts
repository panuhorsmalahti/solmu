import { test, expect } from '@playwright/test'

test('web chat creates a persistent goal and lists it with /goal', async ({ page, request }) => {
  await page.goto('/')
  const composer = page.getByLabel('Message Solmu', { exact: true })
  await expect(composer).toBeEnabled()
  await composer.fill('/goal Prepare the first launch')
  await page.getByRole('button', { name: 'Send message' }).click()
  await expect(page.getByText(/Goal started: Prepare the first launch/)).toBeVisible()
  await composer.fill('/goal')
  await page.getByRole('button', { name: 'Send message' }).click()
  await expect(page.getByText(/\[active\] Prepare the first launch/)).toBeVisible()
  const goals = await (await request.get('/api/v1/goals')).json()
  expect(goals.items).toHaveLength(1)
})
