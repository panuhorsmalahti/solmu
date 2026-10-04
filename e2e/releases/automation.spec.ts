import { test, expect } from '@playwright/test'
import { execFileSync } from 'node:child_process'
import { cpSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const root = resolve('.')
const packages = ['backend', 'clients/common', 'clients/cli', 'clients/desktop', 'boxer', 'muxer', 'muxer-gui', 'e2e', 'cloud/congregator/backend']

function fixture() {
  const directory = mkdtempSync(join(tmpdir(), 'solmu-release-e2e-'))
  for (const path of packages) {
    mkdirSync(join(directory, path), { recursive: true })
    cpSync(join(root, path, 'Cargo.toml'), join(directory, path, 'Cargo.toml'))
  }
  mkdirSync(join(directory, 'scripts'))
  for (const path of ['scripts/release.mjs', 'Cargo.lock', 'package.json', 'package-lock.json', 'clients/web/package.json', 'cloud/congregator/web/package.json']) {
    mkdirSync(join(directory, path, '..'), { recursive: true })
    cpSync(join(root, path), join(directory, path))
  }
  const git = (...args: string[]) => execFileSync('git', args, { cwd: directory, encoding: 'utf8' }).trim()
  git('init', '-b', 'main')
  git('config', 'user.email', 'release-fixture@solmu.invalid')
  git('config', 'user.name', 'Solmu release fixture')
  git('config', 'core.autocrlf', 'false')
  const commit = (message: string) => { git('add', '.'); git('commit', '-m', message) }
  commit('Initial Solmu')
  const env = { ...process.env, GITHUB_REF: 'refs/heads/main', GITHUB_REPOSITORY: 'solmu/fixture', GITHUB_EVENT_NAME: 'schedule', GITHUB_OUTPUT: join(directory, 'outputs'), VERSION: '' }
  const releases = (tag?: string) => writeFileSync(join(directory, 'releases.json'), JSON.stringify(tag ? [{ tag_name: tag, draft: false, prerelease: false }] : []))
  releases()
  const run = (command: string, options = {}) => execFileSync(process.execPath, ['scripts/release.mjs', command, ...(command === 'plan' ? ['--releases-file', 'releases.json'] : [])], { cwd: directory, env: { ...env, ...options }, encoding: 'utf8', stdio: 'pipe' })
  return { directory, git, commit, releases, run, close: () => rmSync(directory, { recursive: true, force: true }) }
}

test('daily release skips unchanged main and increments the patch after new commits', () => {
  const repo = fixture()
  try {
    repo.releases()
    expect(JSON.parse(repo.run('plan'))).toEqual({ release: true, version: '0.1.0', previous: '' })
    repo.git('tag', 'v0.1.0')
    repo.releases('v0.1.0')
    expect(JSON.parse(repo.run('plan')).release).toBe(false)
    writeFileSync(join(repo.directory, 'feature.txt'), 'A new user feature')
    repo.commit('Add a new feature')
    expect(JSON.parse(repo.run('plan'))).toEqual({ release: true, version: '0.1.1', previous: 'v0.1.0' })
    repo.run('notes', { PREVIOUS_TAG: 'v0.1.0' })
    const notes = readFileSync(join(repo.directory, 'artifacts/release-notes.md'), 'utf8')
    expect(notes).toContain('Add a new feature')
    expect(notes).not.toContain('Initial Solmu')
    expect(notes).toContain('https://github.com/solmu/fixture/commit/')
  } finally { repo.close() }
})

test('release stamping updates every artifact manifest and preserves dependency versions', () => {
  const repo = fixture()
  try {
    const before = readFileSync(join(repo.directory, 'Cargo.lock'), 'utf8')
    repo.run('stamp', { VERSION: '0.1.12' })
    for (const path of packages) expect(readFileSync(join(repo.directory, path, 'Cargo.toml'), 'utf8')).toContain('version = "0.1.12"')
    const lock = readFileSync(join(repo.directory, 'Cargo.lock'), 'utf8')
    for (const block of before.split('[[package]]').filter(block => !block.includes('name = "solmu-'))) expect(lock).toContain(block)
    for (const path of ['package.json', 'clients/web/package.json', 'cloud/congregator/web/package.json', 'package-lock.json']) expect(JSON.parse(readFileSync(join(repo.directory, path), 'utf8')).version).toBe('0.1.12')
    expect(JSON.parse(repo.run('plan', { GITHUB_EVENT_NAME: 'workflow_dispatch', VERSION: '0.1.12' }))).toEqual({ release: true, version: '0.1.12', previous: '' })
    expect(() => repo.run('stamp', { VERSION: 'invalid' })).toThrow()
    expect(() => repo.run('plan', { GITHUB_EVENT_NAME: 'workflow_dispatch', VERSION: '0.1.9' })).toThrow(/Bump/)
    expect(() => repo.run('plan', { GITHUB_REF: 'refs/heads/other' })).toThrow(/main/)
  } finally { repo.close() }
})

test('daily release fails on an unreachable previous tag and uses a deliberate version bump', () => {
  const repo = fixture()
  try {
    repo.releases('v9.0.0')
    expect(() => repo.run('plan')).toThrow()
    repo.releases()
    repo.git('tag', 'v0.1.0')
    repo.releases('v0.1.0')
    repo.run('stamp', { VERSION: '0.2.0' })
    repo.commit('Prepare the next minor release')
    expect(JSON.parse(repo.run('plan')).version).toBe('0.2.0')
  } finally { repo.close() }
})

test('release workflow combines the commit changelog with generated release notes', () => {
  const workflow = readFileSync(join(root, '.github/workflows/release.yml'), 'utf8')
  expect(workflow).toContain('--notes "$(cat release-notes.md)" --generate-notes')
  expect(workflow).not.toContain('--notes-file release-notes.md --generate-notes')
})
