import { expect, test } from '@playwright/test'
import { readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { releaseFixture } from '../install/support'

for (const component of ['backend', 'bundle']) {
  test(`${component} registers a background backend and preserves provider configuration on upgrade`, async () => {
    const fixture = await releaseFixture()
    try {
      await fixture.run(component, { service: true })
      const state = path.join(fixture.directory, 'state')
      expect(await readFile(path.join(state, '.env'), 'utf8')).toContain('LLM_PROVIDER=openai')
      expect(await readFile(path.join(state, '.env'), 'utf8')).toContain('SOLMU_AUTO_UPDATE=true')
      expect(await readFile(path.join(state, '.solmu-backend-version'), 'utf8')).toBe('v0.1.0')
      expect(await readFile(path.join(state, `auto-update.${process.platform === 'win32' ? 'ps1' : 'sh'}`), 'utf8')).toMatch(/SOLMU_AUTO_UPDATE/)
      const configuration = 'LLM_PROVIDER=openai\nOPENAI_API_KEY=user-provided-key\n'
      await writeFile(path.join(state, '.env'), configuration)
      await fixture.run(component, { service: true })
      expect(await readFile(path.join(state, '.env'), 'utf8')).toBe(configuration)
      const record = await readFile(path.join(fixture.directory, 'service-record'), 'utf8')
      if (process.platform === 'win32') {
        expect(record).toContain('stop')
        expect(record).toContain('register')
        expect(record).toContain('start')
        const task = JSON.parse(await readFile(path.join(fixture.directory, 'service-record.Solmu Backend.json'), 'utf8'))
        expect(task.Name).toBe('Solmu Backend')
        expect(task.Trigger.AtLogOn).toBe(true)
        expect(task.Principal.RunLevel).toBe('Limited')
        expect(task.Action.Argument).toContain('-WindowStyle Hidden')
        expect(task.Action.Argument).toContain(fixture.destination)
        const updateTask = JSON.parse(await readFile(path.join(fixture.directory, 'service-record.Solmu Auto Update.json'), 'utf8'))
        expect(updateTask.Trigger.Daily).toBe(true)
        expect(updateTask.Action.Argument).toContain('auto-update.ps1')
        const runner = await readFile(path.join(state, 'backend-service.ps1'), 'utf8')
        expect(runner).toContain('KILL_ON_JOB_CLOSE')
        expect(runner).toContain('$start.CreateNoWindow = $true')
      } else if (process.platform === 'darwin') {
        expect(record).toContain('bootstrap')
        expect(record).toContain('kickstart')
        const plist = await readFile(path.join(fixture.directory, 'home', 'Library', 'LaunchAgents', 'dev.solmu.backend.plist'), 'utf8')
        expect(plist).toContain(fixture.destination)
        expect(plist).toContain('<key>KeepAlive</key><true/>')
        const updater = await readFile(path.join(fixture.directory, 'home', 'Library', 'LaunchAgents', 'dev.solmu.autoupdate.plist'), 'utf8')
        expect(updater).toContain('<key>StartInterval</key><integer>86400</integer>')
      } else {
        expect(record).toContain('--user stop solmu-backend.service')
        expect(record).toContain('--user enable --now solmu-backend.service')
        const unit = await readFile(path.join(fixture.directory, 'home', '.config', 'systemd', 'user', 'solmu-backend.service'), 'utf8')
        expect(unit).toContain(fixture.destination)
        expect(unit).toContain('Restart=on-failure')
        expect(unit).toContain('WorkingDirectory="' + state + '"')
        const timer = await readFile(path.join(fixture.directory, 'home', '.config', 'systemd', 'user', 'solmu-auto-update.timer'), 'utf8')
        expect(timer).toContain('OnCalendar=daily')
        expect(record).toContain('--user enable --now solmu-auto-update.timer')
      }
      const previousRequests = fixture.requests.length
      await writeFile(path.join(state, '.env'), configuration + 'SOLMU_AUTO_UPDATE=false\n')
      await fixture.runAutoUpdate()
      expect(fixture.requests).toHaveLength(previousRequests)
      await writeFile(path.join(state, '.env'), configuration + 'SOLMU_AUTO_UPDATE=true\n')
      await fixture.runAutoUpdate()
      expect(fixture.requests.slice(previousRequests)).toContain('/api/latest')
      expect(fixture.requests.slice(previousRequests)).not.toContain('/download/v0.1.0/solmu-v0.1.0-' + (process.platform === 'win32' ? 'windows-x86_64.zip' : process.platform === 'darwin' ? `macos-${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}.tar.gz` : 'linux-x86_64.tar.gz'))
    } finally { await fixture.close() }
  })
}
