import { test, expect } from '@playwright/test'
import { mkdir, mkdtemp, writeFile, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

test('workspace skills update live, report invalid files, and preserve an unsent conversation draft', async ({ page, request }) => {
  const workspace = await mkdtemp(join(tmpdir(), 'solmu-web-skills-'))
  const thread = await (await request.post('/api/v1/threads', { data: { title: 'Workspace skills', workspace } })).json()
  const skill = join(workspace, '.agents/skills/writing/SKILL.md')
  try {
    await page.goto(`/threads/${thread.id}`)
    await expect(page.getByLabel('Message Solmu')).toBeEnabled()
    await page.getByLabel('Message Solmu').fill('Keep my unsent draft')
    await page.getByRole('button', { name: 'Skills', exact: true }).click()
    await expect(page.getByText('No skills installed in this workspace.')).toBeVisible()
    await mkdir(join(workspace, '.agents/skills/writing'), { recursive: true })
    await writeFile(skill, '---\nname: writing\ndescription: Write clear explanations with practical examples.\n---\nUse concrete examples.\n')
    await expect(page.getByRole('heading', { name: 'writing', exact: true })).toBeVisible()
    await expect(page.getByText('Write clear explanations with practical examples.', { exact: true })).toBeVisible()
    if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
      await page.setViewportSize({ width: 1440, height: 1000 })
      await page.screenshot({ path: 'docs/screenshots/web-skills.png' })
    }
    await writeFile(skill, '---\nname: writing\ndescription: Updated explanations.\n---\nUpdated instructions.\n')
    await expect(page.getByText('Updated explanations.', { exact: true })).toBeVisible()
    await writeFile(skill, '---\nname: mismatched\ndescription: Not valid.\n---\n')
    await expect(page.getByText('name must match the skill directory name', { exact: true })).toBeVisible()
    await page.getByRole('button', { name: 'Back to conversation' }).click()
    await expect(page.getByLabel('Message Solmu')).toHaveValue('Keep my unsent draft')
    expect((await (await request.get(`/api/v1/threads/${thread.id}/messages`)).json()).items).toHaveLength(0)
    const second = await (await request.post('/api/v1/threads', { data: { title: 'Other skill workspace' } })).json()
    try {
      await page.getByLabel('Thread selector').getByRole('button', { name: 'Other skill workspace', exact: true }).click()
      await page.getByRole('button', { name: 'Skills', exact: true }).click()
      await expect(page.getByText('No skills installed in this workspace.')).toBeVisible()
      await expect(page.getByText('name must match the skill directory name', { exact: true })).not.toBeVisible()
    } finally { await request.delete(`/api/v1/threads/${second.id}`) }
  } finally {
    await request.delete(`/api/v1/threads/${thread.id}`)
    await rm(workspace, { recursive: true, force: true })
  }
})
