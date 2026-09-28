import { test, expect } from '@playwright/test'
import { mkdtemp, writeFile, rm } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { tmpdir } from 'node:os'

test('MCP status and tools update live and preserve an unsent conversation draft', async ({ page, request }) => {
  const workspace = await mkdtemp(join(tmpdir(), 'solmu-web-mcp-'))
  const thread = await (await request.post('/api/v1/threads', { data: { title: 'Connected project tools', workspace } })).json()
  const config = join(workspace, '.mcp.json')
  try {
    await page.goto(`/threads/${thread.id}`)
    await expect(page.getByLabel('Message Solmu')).toBeEnabled()
    await page.getByLabel('Message Solmu').fill('Keep my unsent MCP draft')
    await page.getByRole('button', { name: 'MCP', exact: true }).click()
    await expect(page.getByText('No MCP servers configured in this workspace.')).toBeVisible()
    const binary = resolve(`target/debug/mcp-fixture${process.platform === 'win32' ? '.exe' : ''}`)
    await writeFile(config, JSON.stringify({ mcpServers: { notes: { command: binary }, calendar: { disabled: true } } }))
    await expect(page.getByText('Search project notes with MCP.', { exact: true })).toBeVisible()
    await expect(page.getByText('connected · stdio · 3 tools', { exact: true })).toBeVisible()
    if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
      await page.setViewportSize({ width: 1440, height: 1000 })
      await page.screenshot({ path: 'docs/screenshots/web-mcp.png' })
    }
    await writeFile(join(workspace, 'mcp-description.txt'), 'Updated MCP notes.')
    await expect(page.getByText('Updated MCP notes.', { exact: true })).toBeVisible({ timeout: 12000 })
    await writeFile(config, 'invalid JSON')
    await expect(page.getByText('MCP config must be valid JSON')).toBeVisible()
    await page.getByRole('button', { name: 'Back to conversation' }).click()
    await expect(page.getByLabel('Message Solmu')).toHaveValue('Keep my unsent MCP draft')
    expect((await (await request.get(`/api/v1/threads/${thread.id}/messages`)).json()).items).toHaveLength(0)
  } finally {
    await request.delete(`/api/v1/threads/${thread.id}`)
    await rm(workspace, { recursive: true, force: true })
  }
})
