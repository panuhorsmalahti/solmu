import { expect, test } from '@playwright/test'
import { readFile, readdir } from 'node:fs/promises'
import path from 'node:path'
import { releaseFixture } from './support'

for (const corrupt of [false, true]) {
  test(corrupt ? 'bundle rejects corrupt downloads before installing' : 'separate bundle installs backend and every client', async () => {
    const fixture = await releaseFixture({ corrupt })
    try {
      if (corrupt) {
        await expect(fixture.run('bundle')).rejects.toThrow(/checksum mismatch/)
        await expect(readdir(fixture.destination)).rejects.toThrow()
      } else {
        expect((await fixture.run('bundle')).stdout).toContain('Installed Solmu v0.1.0 (all)')
        expect((await readdir(fixture.destination)).sort()).toEqual(Object.keys(fixture.native).sort())
        for (const [name, content] of Object.entries(fixture.native)) expect(await readFile(path.join(fixture.destination, name))).toEqual(Buffer.from(content))
        expect(await readFile(path.join(fixture.directory, 'state', 'web', 'index.html'), 'utf8')).toContain('Solmu web')
        await fixture.run('bundle')
      }
    } finally { await fixture.close() }
  })
}
