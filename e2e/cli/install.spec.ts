import { nativeInstallerTests } from '../install/support'
nativeInstallerTests('cli', ['solmu'])

import { expect, test } from '@playwright/test'
import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { releaseFixture, executable } from '../install/support'

test('older published releases install the CLI as solmu and preserve their runtime compatibility', async () => {
  const fixture = await releaseFixture({ legacyCli: true })
  try {
    await fixture.run('cli')
    expect(await readFile(path.join(fixture.destination, executable('solmu')), 'utf8')).toContain('Published solmu')
    expect(await readFile(path.join(fixture.destination, executable('solmu-cli')), 'utf8')).toContain('Published solmu')
  } finally { await fixture.close() }
})
