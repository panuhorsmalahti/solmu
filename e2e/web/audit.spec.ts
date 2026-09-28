import { test, expect } from '@playwright/test'

test('Audit follows Profile, expands saved calls, loads older pages, and updates live', async ({ page, request }) => {
  const thread = await (await request.post('/api/v1/threads', { data: { title: 'Audit project' } })).json()
  async function tools() {
    const message = await (await request.post(`/api/v1/threads/${thread.id}/messages`, { data: { content: 'TOOLS' } })).json()
    const response = await request.post(`/api/v1/threads/${thread.id}/responses`, { data: { message_id: message.id }, timeout: 30_000 })
    expect(response.ok()).toBe(true)
    expect(await response.text()).toContain('event: done')
  }
  try {
    for (let index = 0; index < 5; index++) await tools()
    await page.goto('/audit')
    await expect(page.getByRole('heading', { name: 'Audit' })).toBeVisible()
    const sidebar = page.locator('.sidebar')
    await expect(sidebar.getByRole('button', { name: 'Profile', exact: true })).toBeVisible()
    await expect(sidebar.getByRole('button', { name: 'Audit', exact: true })).toBeVisible()
    await expect(sidebar.locator('.profile-link + .audit-link')).toBeVisible()
    await expect(page.locator('.audit-card')).toHaveCount(25)
    await page.locator('.audit-card summary').first().click()
    await expect(page.locator('.audit-card').first().getByText('Arguments', { exact: true })).toBeVisible()
    await expect(page.locator('.audit-card').first().getByText('Result', { exact: true })).toBeVisible()
    if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
      await page.setViewportSize({ width: 1440, height: 1000 })
      await page.screenshot({ path: 'docs/screenshots/web-audit.png' })
    }
    await page.locator('.audit-page').evaluate(element => { element.scrollTop = element.scrollHeight })
    await expect(page.locator('.audit-card')).toHaveCount(30)
    const first = await page.locator('.audit-card').first().getAttribute('data-audit-id')
    await tools()
    await expect.poll(() => page.locator('.audit-card').first().getAttribute('data-audit-id')).not.toBe(first)
    await request.delete(`/api/v1/threads/${thread.id}`)
    await expect(page.getByText('No tool calls yet.')).toBeVisible()
  } finally {
    await request.delete(`/api/v1/threads/${thread.id}`)
  }
})
