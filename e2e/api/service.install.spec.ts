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
      const configuration = 'LLM_PROVIDER=openai\nOPENAI_API_KEY=user-provided-key\n'
      await writeFile(path.join(state, '.env'), configuration)
      await fixture.run(component, { service: true })
      expect(await readFile(path.join(state, '.env'), 'utf8')).toBe(configuration)
      const record = await readFile(path.join(fixture.directory, 'service-record'), 'utf8')
      if (process.platform === 'win32') {
        expect(record).toContain('stop')
        expect(record).toContain('register')
        expect(record).toContain('start')
        const task = JSON.parse(await readFile(path.join(fixture.directory, 'service-record.json'), 'utf8'))
        expect(task.Name).toBe('Solmu Backend')
        expect(task.Trigger.AtLogOn).toBe(true)
        expect(task.Principal.RunLevel).toBe('Limited')
        expect(task.Action.Argument).toContain('-WindowStyle Hidden')
        expect(task.Action.Argument).toContain(fixture.destination)
        const runner = await readFile(path.join(state, 'backend-service.ps1'), 'utf8')
        expect(runner).toContain('KILL_ON_JOB_CLOSE')
        expect(runner).toContain('$start.CreateNoWindow = $true')
      } else if (process.platform === 'darwin') {
        expect(record).toContain('bootstrap')
        expect(record).toContain('kickstart')
        const plist = await readFile(path.join(fixture.directory, 'home', 'Library', 'LaunchAgents', 'dev.solmu.backend.plist'), 'utf8')
        expect(plist).toContain(fixture.destination)
        expect(plist).toContain('<key>KeepAlive</key><true/>')
      } else {
        expect(record).toContain('--user stop solmu-backend.service')
        expect(record).toContain('--user enable --now solmu-backend.service')
        const unit = await readFile(path.join(fixture.directory, 'home', '.config', 'systemd', 'user', 'solmu-backend.service'), 'utf8')
        expect(unit).toContain(fixture.destination)
        expect(unit).toContain('Restart=on-failure')
        expect(unit).toContain('WorkingDirectory="' + state + '"')
      }
    } finally { await fixture.close() }
  })
}
