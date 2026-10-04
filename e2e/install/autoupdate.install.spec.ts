import { expect, test } from '@playwright/test'
import { readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { releaseFixture } from './support'

const installedModules: Record<string, string[]> = {
  cli: ['cli'], desktop: ['desktop'], web: ['web'], boxer: ['boxer'], muxer: ['cli', 'muxer'], 'muxer-gui': ['cli', 'muxer', 'muxer-gui'],
}

for (const [component, modules] of Object.entries(installedModules)) {
  test(`${component} installer registers only its installed modules for automatic updates`, async () => {
    const fixture = await releaseFixture()
    try {
      await fixture.run(component, { service: true })
      const state = path.join(fixture.directory, 'state')
      for (const module of modules) {
        const destination = fixture.destination
        expect((await readFile(path.join(state, 'update-components', module), 'utf8')).trim()).toBe(destination)
        expect((await readFile(path.join(state, `.solmu-version-${module}`), 'utf8')).trim()).toBe('v0.1.0')
      }
      for (const module of ['backend', 'cli', 'desktop', 'web', 'boxer', 'muxer', 'muxer-gui']) {
        if (!modules.includes(module)) await expect(readFile(path.join(state, 'update-components', module))).rejects.toThrow()
      }
      if (process.platform === 'win32') {
        const task = JSON.parse(await readFile(path.join(fixture.directory, 'service-record.Solmu Auto Update.json'), 'utf8'))
        expect(task.Trigger.Daily).toBe(true)
        expect(task.Action.Argument).toContain('auto-update-v2.ps1')
      } else if (process.platform === 'darwin') {
        expect(await readFile(path.join(fixture.directory, 'home', 'Library', 'LaunchAgents', 'dev.solmu.autoupdate.plist'), 'utf8')).toContain('<key>StartInterval</key><integer>86400</integer>')
      } else {
        expect(await readFile(path.join(fixture.directory, 'home', '.config', 'systemd', 'user', 'solmu-auto-update.timer'), 'utf8')).toContain('OnCalendar=daily')
      }
    } finally { await fixture.close() }
  })
}

test('automatic updates honor the shared opt-out and update an installed CLI only', async () => {
  const fixture = await releaseFixture()
  try {
    await fixture.run('cli', { service: true })
    const state = path.join(fixture.directory, 'state')
    const configuration = path.join(state, '.env')
    await writeFile(configuration, 'SOLMU_AUTO_UPDATE=false\n')
    const requestCount = fixture.requests.length
    await fixture.runAutoUpdate()
    expect(fixture.requests).toHaveLength(requestCount)

    await writeFile(configuration, 'SOLMU_AUTO_UPDATE=true\n')
    await writeFile(path.join(state, '.solmu-version-cli'), 'v0.0.9')
    await fixture.runAutoUpdate()
    expect((await readFile(path.join(state, '.solmu-version-cli'), 'utf8')).trim()).toBe('v0.1.0')
    expect(fixture.requests).toContain('/api/latest')
    expect(fixture.requests.some(request => request.includes('solmu-v0.1.0-'))).toBe(true)
    expect(fixture.requests.some(request => request.includes('solmu-v0.1.0-web.zip'))).toBe(false)
    await expect(readFile(path.join(state, 'update-components', 'desktop'))).rejects.toThrow()
  } finally { await fixture.close() }
})
