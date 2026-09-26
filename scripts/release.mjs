import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'

const packages = ['backend', 'clients/common', 'clients/cli', 'clients/desktop', 'boxer', 'muxer', 'e2e']
const semver = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/
const read = path => readFileSync(path, 'utf8')
const git = (...args) => execFileSync('git', args, { encoding: 'utf8' }).trim()
function version(value) {
  if (!semver.test(value)) throw new Error(`Invalid release version: ${value}`)
  return value.split('.').map(Number)
}
function compare(left, right) {
  const a = version(left), b = version(right)
  for (let index = 0; index < 3; index++) if (a[index] !== b[index]) return a[index] - b[index]
  return 0
}
function packageVersion(path) {
  const value = read(`${path}/Cargo.toml`).match(/^version\s*=\s*"([^"]+)"/m)?.[1]
  version(value)
  return value
}
function output(plan) {
  const content = Object.entries(plan).map(([key, value]) => `${key}=${value}`).join('\n') + '\n'
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, content)
  console.log(JSON.stringify(plan))
}

const [command, ...args] = process.argv.slice(2)
if (command === 'plan') {
  const repository = process.env.GITHUB_REPOSITORY
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? '')) throw new Error('GITHUB_REPOSITORY must name the release repository')
  if (process.env.GITHUB_REF !== 'refs/heads/main') throw new Error('Production releases must run from main')
  // A fixture file exercises the real git/version logic without publishing.
  const fixture = args.indexOf('--releases-file')
  const releases = fixture >= 0 ? JSON.parse(read(args[fixture + 1])) : JSON.parse(execFileSync('gh', ['api', `repos/${repository}/releases?per_page=100`], { encoding: 'utf8' }))
  const stable = releases.filter(release => !release.draft && !release.prerelease && /^v\d+\.\d+\.\d+$/.test(release.tag_name))
    .sort((a, b) => compare(b.tag_name.slice(1), a.tag_name.slice(1)))
  const previous = stable[0]?.tag_name ?? ''
  if (previous) {
    // Require a reachable tag. An API/network failure must never look like a first release.
    git('merge-base', '--is-ancestor', previous, 'HEAD')
    if (process.env.GITHUB_EVENT_NAME === 'schedule' && git('rev-list', '--count', `${previous}..HEAD`) === '0') {
      output({ release: false, version: '', previous })
      process.exit(0)
    }
  }
  let selected = process.env.VERSION
  if (process.env.GITHUB_EVENT_NAME === 'schedule') {
    const committed = packageVersion('backend')
    if (!previous || compare(committed, previous.slice(1)) > 0) selected = committed
    else { const next = version(previous.slice(1)); next[2]++; selected = next.join('.') }
  } else {
    for (const path of packages) if (packageVersion(path) !== selected) throw new Error(`Bump ${path}/Cargo.toml to ${selected} before releasing`)
  }
  version(selected)
  const existing = git('tag', '--list', `v${selected}`)
  if (existing) throw new Error('This release tag already exists')
  output({ release: true, version: selected, previous })
} else if (command === 'stamp') {
  const selected = process.env.VERSION
  version(selected)
  const names = packages.map(path => read(`${path}/Cargo.toml`).match(/^name\s*=\s*"([^"]+)"/m)[1])
  for (const path of packages) writeFileSync(`${path}/Cargo.toml`, read(`${path}/Cargo.toml`).replace(/^version\s*=\s*"[^"]+"/m, `version = "${selected}"`))
  writeFileSync('Cargo.lock', read('Cargo.lock').replace(/(\[\[package\]\]\s*name = "([^"]+)"\s*version = ")[^"]+(")/g, (full, prefix, name, suffix) => names.includes(name) ? `${prefix}${selected}${suffix}` : full))
  for (const path of ['package.json', 'clients/web/package.json']) {
    const data = JSON.parse(read(path)); data.version = selected
    writeFileSync(path, JSON.stringify(data, null, 2) + '\n')
  }
  const lock = JSON.parse(read('package-lock.json'))
  lock.version = selected
  for (const path of ['', 'clients/web']) lock.packages[path].version = selected
  writeFileSync('package-lock.json', JSON.stringify(lock, null, 2) + '\n')
} else if (command === 'notes') {
  const previous = process.env.PREVIOUS_TAG
  if (previous && !/^v\d+\.\d+\.\d+$/.test(previous)) throw new Error('Invalid previous release tag')
  const commits = git('log', '--format=%H%x09%s', previous ? `${previous}..HEAD` : 'HEAD').split('\n').filter(Boolean)
  const escape = subject => subject.replace(/[\[\]_*`<>]/g, character => `\\${character}`)
  const notes = '## Changes\n\n' + commits.map(line => {
    const [sha, ...subject] = line.split('\t')
    return `- ${escape(subject.join(' '))} ([${sha.slice(0, 7)}](https://github.com/${process.env.GITHUB_REPOSITORY}/commit/${sha}))`
  }).join('\n') + '\n'
  mkdirSync('artifacts', { recursive: true })
  writeFileSync('artifacts/release-notes.md', notes)
} else throw new Error('Use release.mjs plan, stamp, or notes')
