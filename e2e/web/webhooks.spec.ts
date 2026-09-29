import { test, expect } from '@playwright/test'
import { createHmac } from 'node:crypto'
import { join } from 'node:path'

test('webhooks can be authenticated, enabled, edited live, and trigger a streamed agent turn', async ({ page, request }) => {
  await page.setViewportSize({ width: 1440, height: 1600 })
  await page.goto('/webhooks')
  await expect(page.getByRole('heading', { name: 'Webhooks', exact: true })).toBeVisible()
  await page.getByLabel('Name', { exact: true }).fill('GitHub E2E')
  await page.getByLabel('Secret', { exact: true }).fill('github-e2e-secret-with-entropy')
  await page.getByLabel('Instructions for Solmu').fill('Summarize this event for the project owner.')
  await page.getByRole('button', { name: 'Create webhook' }).click()
  const hook = page.locator('.webhook-card').first()
  await expect(hook).toBeVisible()
  const endpoint = await hook.locator('.webhook-endpoint').innerText()
  expect(endpoint).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/hooks\//)
  await expect(hook).toContainText('Disabled')

  if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
    await page.screenshot({ path: join(process.cwd(), 'docs/screenshots/web-webhooks.png'), fullPage: true })
  }

  await page.getByLabel('Enable GitHub E2E').check()
  await expect(hook).toContainText('Enabled')
  const id = endpoint.split('/').at(-1)!
  const config = await (await request.get(`/api/v1/webhooks`)).json()
  expect(config.find((item: { id: string }) => item.id === id).enabled).toBe(true)
  expect(JSON.stringify(config)).not.toContain('github-e2e-secret-with-entropy')

  await hook.getByLabel('Instructions for Solmu').fill('Keep my unsaved change.')
  await request.patch(`/api/v1/webhooks/${id}`, { data: { name: 'GitHub E2E updated' } })
  await expect(hook.getByRole('heading', { name: 'GitHub E2E updated' })).toBeVisible()
  await expect(hook.getByLabel('Instructions for Solmu')).toHaveValue('Keep my unsaved change.')
  await hook.getByRole('button', { name: 'Save changes' }).click()

  const body = JSON.stringify({ action: 'opened', issue: { title: 'Review the new feature' } })
  const signature = `sha256=${createHmac('sha256', 'github-e2e-secret-with-entropy').update(body).digest('hex')}`
  const delivery = await request.post(endpoint, {
    data: body,
    headers: { 'content-type': 'application/json', 'x-hub-signature-256': signature, 'x-github-event': 'issues', 'x-github-delivery': 'web-e2e-delivery' },
  })
  expect(delivery.status()).toBe(202)
  const threadId = (await delivery.json()).thread_id as string
  await expect.poll(async () => (await (await request.get(`/api/v1/threads/${threadId}/messages`)).json()).items.length, { timeout: 15_000 }).toBe(2)
  const messages = (await (await request.get(`/api/v1/threads/${threadId}/messages`)).json()).items
  expect(messages[0].content).toContain('Review the new feature')
  expect(messages[0].content).toContain('Keep my unsaved change.')
  expect(messages[1].role).toBe('assistant')

  await hook.getByLabel('Enable GitHub E2E updated').uncheck()
  const disabled = await request.post(endpoint, { data: body, headers: { 'content-type': 'application/json', 'x-hub-signature-256': signature } })
  expect(disabled.status()).toBe(404)
  await page.getByRole('button', { name: 'Delete GitHub E2E updated' }).click()
  await request.delete(`/api/v1/threads/${threadId}`)
})
