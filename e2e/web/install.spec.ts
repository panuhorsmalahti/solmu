import { test, expect } from '@playwright/test'
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { strToU8 } from 'fflate'
import { releaseFixture } from '../install/support'

test('web installer extracts published site files without installing native binaries', async () => {
  const fixture = await releaseFixture()
  try {
    expect((await fixture.run('web')).stdout).toContain('Installed Solmu web v0.1.0')
    expect((await readdir(fixture.destination)).sort()).toEqual(['.solmu-web', 'assets', 'index.html'])
    for (const [name, content] of Object.entries(fixture.web)) expect(await readFile(path.join(fixture.destination, name))).toEqual(Buffer.from(content))
  } finally { await fixture.close() }
})

test('web installer defaults to the backend web folder and supports pinned versions', async () => {
  const fixture = await releaseFixture()
  try {
    await fixture.run('web', { pinned: true, useDefaultDestination: true })
    expect(await readFile(path.join(fixture.directory, 'state', 'web', 'index.html'), 'utf8')).toContain('Solmu web')
    expect(fixture.requests).not.toContain('/api/latest')
  } finally { await fixture.close() }
})

test('web installer preserves an existing destination', async () => {
  const fixture = await releaseFixture()
  try {
    await mkdir(fixture.destination)
    await writeFile(path.join(fixture.destination, 'index.html'), 'My existing deployment')
    await expect(fixture.run('web')).rejects.toThrow(/already exists/)
    expect(await readFile(path.join(fixture.destination, 'index.html'), 'utf8')).toBe('My existing deployment')
    expect(fixture.requests).not.toContain('/api/latest')
  } finally { await fixture.close() }
})

test('web installer rejects corrupt downloads', async () => {
  const fixture = await releaseFixture({ corrupt: true })
  try {
    await expect(fixture.run('web')).rejects.toThrow(/checksum mismatch/)
    await expect(readdir(fixture.destination)).rejects.toThrow()
  } finally { await fixture.close() }
})

test('web installer rejects archive paths outside its destination', async () => {
  const fixture = await releaseFixture({ webFiles: { 'index.html': strToU8('Solmu'), '../escaped.txt': strToU8('Must not escape') } })
  try {
    await expect(fixture.run('web')).rejects.toThrow(/Unsafe path/)
    await expect(readdir(fixture.destination)).rejects.toThrow()
    await expect(readFile(path.join(fixture.directory, 'escaped.txt'))).rejects.toThrow()
  } finally { await fixture.close() }
})

test('web installer requires the published index document', async () => {
  const fixture = await releaseFixture({ webFiles: { 'assets/app.js': strToU8('Solmu') } })
  try {
    await expect(fixture.run('web')).rejects.toThrow(/Missing web index.html/)
    await expect(readdir(fixture.destination)).rejects.toThrow()
  } finally { await fixture.close() }
})

test('web upgrades preserve old assets and update the index in a managed deployment', async () => {
  const fixture = await releaseFixture()
  try {
    await fixture.run('web')
    await writeFile(path.join(fixture.destination, 'assets', 'previous.js'), 'Old tab asset')
    await writeFile(path.join(fixture.destination, 'index.html'), 'Old index')
    await fixture.run('web')
    expect(await readFile(path.join(fixture.destination, 'index.html'), 'utf8')).toContain('Solmu web')
    expect(await readFile(path.join(fixture.destination, 'assets', 'previous.js'), 'utf8')).toBe('Old tab asset')
  } finally { await fixture.close() }
})
