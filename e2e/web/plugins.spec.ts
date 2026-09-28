import { test, expect } from '@playwright/test'
import { mkdtemp, writeFile, mkdir, rm, copyFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { tmpdir } from 'node:os'

test('plugins update live and preserve the conversation draft', async ({ page, request }) => {
  const workspace = await mkdtemp(join(tmpdir(), 'solmu-web-plugins-'))
  const thread = await (await request.post('/api/v1/threads', { data: { title: 'Project plugins', workspace } })).json()
  const root = join(workspace, '.agents', 'plugins', 'project-tools')
  try {
    await page.goto(`/threads/${thread.id}`)
    await expect(page.getByLabel('Message Solmu')).toBeEnabled()
    await page.getByLabel('Message Solmu').fill('Keep my unsent plugin draft')
    await page.getByRole('button', { name: 'Plugins', exact: true }).click()
    await expect(page.getByText('No plugins installed in this workspace.')).toBeVisible()
    await mkdir(root, { recursive: true })
    const manifest = join(root, 'plugin.json')
    await writeFile(manifest, JSON.stringify({ '$schema': 'https://agent-plugins.org/schemas/1.0.0/plugin.schema.json', name: 'project-tools', description: 'Tools for this project' }))
    await expect(page.getByText('Tools for this project')).toBeVisible()
    await mkdir(join(root, 'skills', 'project-notes'), { recursive: true })
    await writeFile(join(root, 'skills', 'project-notes', 'SKILL.md'), '---\nname: project-notes\ndescription: Read project notes from a plugin.\n---\n\nUse the project MCP tool.\n')
    await mkdir(join(root, 'bin'), { recursive: true })
    const program = `mcp-fixture${process.platform === 'win32' ? '.exe' : ''}`
    await copyFile(resolve('target/debug', program), join(root, 'bin', program))
    await writeFile(join(root, 'mcp.json'), JSON.stringify({ '$schema': 'https://agent-plugins.org/schemas/1.0.0/mcp.schema.json', mcpServers: { notes: { type: 'stdio', command: `./bin/${program}` } } }))
    await expect(page.getByText('1 skill · 1 MCP server')).toBeVisible()
    if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
      await page.setViewportSize({ width: 1440, height: 1000 })
      await page.screenshot({ path: 'docs/screenshots/web-plugins.png' })
    }
    await writeFile(manifest, 'invalid JSON')
    await expect(page.getByText('No plugins installed in this workspace.')).toBeVisible()
    await expect(page.getByText('Not loaded:', { exact: false })).toBeVisible()
    await page.getByRole('button', { name: 'Back to conversation' }).click()
    await expect(page.getByLabel('Message Solmu')).toHaveValue('Keep my unsent plugin draft')
    expect((await (await request.get(`/api/v1/threads/${thread.id}/messages`)).json()).items).toHaveLength(0)
  } finally {
    await request.delete(`/api/v1/threads/${thread.id}`)
    await rm(workspace, { recursive: true, force: true })
  }
})
