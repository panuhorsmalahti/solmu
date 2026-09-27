import { expect, test } from '@playwright/test'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { releaseFixture } from '../install/support'

test('Unix installation updates the existing login profile without shadowing its settings', async () => {
  test.skip(process.platform === 'win32', 'Unix login shell configuration')
  const fixture = await releaseFixture()
  try {
    const home = path.join(fixture.directory, 'home')
    await mkdir(home)
    const profile = path.join(home, '.profile')
    await writeFile(profile, 'export SOLMU_PROFILE_SENTINEL=preserved\n')
    await fixture.run('cli', { persistPath: true })
    await fixture.run('cli', { persistPath: true })
    const contents = await readFile(profile, 'utf8')
    expect(contents).toContain('export SOLMU_PROFILE_SENTINEL=preserved')
    expect(contents.split('# Solmu installed programs').length).toBe(2)
    await expect(readFile(path.join(home, '.bash_profile'))).rejects.toThrow()
    const output = await promisify(execFile)('sh', ['-c', '. "$1"; printf "%s\\n" "$SOLMU_PROFILE_SENTINEL"; case ":$PATH:" in *":$2:"*) exit 0 ;; *) exit 1 ;; esac', 'solmu-path-check', profile, fixture.destination], { env: { ...process.env, HOME: home } })
    expect(output.stdout.trim()).toBe('preserved')
  } finally { await fixture.close() }
})
