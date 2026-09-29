import { expect, test } from '@playwright/test'
import { createServer } from 'node:http'
import { createHash } from 'node:crypto'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { mkdtemp, readFile, readdir, rm, mkdir, writeFile, chmod, realpath } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { gzipSync, zipSync, strToU8 } from 'fflate'

const execute = promisify(execFile)
const windows = process.platform === 'win32'
export const executable = (name: string) => name + (windows ? '.exe' : '')

function tarball(files: Record<string, Uint8Array>) {
  const parts: Buffer[] = []
  for (const [name, data] of Object.entries(files)) {
    const header = Buffer.alloc(512)
    header.write(name, 0)
    header.write('0000755\0', 100)
    header.write('0000000\0', 108)
    header.write('0000000\0', 116)
    header.write(data.length.toString(8).padStart(11, '0') + '\0', 124)
    header.write('00000000000\0', 136)
    header.fill(32, 148, 156)
    header.write('0', 156)
    header.write('ustar\0', 257)
    header.write('00', 263)
    header.write(header.reduce((sum, value) => sum + value, 0).toString(8).padStart(6, '0') + '\0 ', 148)
    parts.push(header, Buffer.from(data), Buffer.alloc((512 - data.length % 512) % 512))
  }
  return gzipSync(Buffer.concat([...parts, Buffer.alloc(1024)]))
}

export async function releaseFixture(options: { corrupt?: boolean; webFiles?: Record<string, Uint8Array>; missingScript?: boolean; legacyCli?: boolean } = {}) {
  const directory = await realpath(await mkdtemp(path.join(tmpdir(), 'solmu-client-install-e2e-')))
  const destination = path.join(directory, 'install with spaces')
  const native = Object.fromEntries(['solmu-backend', 'solmu', 'solmu-desktop', 'boxer', 'muxer'].map(name => [executable(name), strToU8(`Published ${name}\n`)]))
  if (options.legacyCli) { native[executable('solmu-cli')] = native[executable('solmu')]; delete native[executable('solmu')] }
  const web = options.webFiles ?? { 'index.html': strToU8('<html>Solmu web</html>'), 'assets/app.js': strToU8('console.log("Solmu")'), 'assets/app.css': strToU8('body{color:green}') }
  const platform = windows ? 'windows-x86_64' : process.platform === 'darwin' ? `macos-${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}` : 'linux-x86_64'
  const nativeAsset = `solmu-v0.1.0-${platform}.${windows ? 'zip' : 'tar.gz'}`
  const webAsset = 'solmu-v0.1.0-web.zip'
  const archives = { [nativeAsset]: windows ? zipSync(native) : tarball(native), [webAsset]: zipSync(web) }
  const checksums = Object.entries(archives).map(([asset, data]) => `${options.corrupt ? '0'.repeat(64) : createHash('sha256').update(data).digest('hex')}  ${asset}\n`).join('')
  const helpers = Object.fromEntries(await Promise.all(['sh', 'ps1'].map(async extension => [extension, await readFile(`scripts/service.${extension}`)])))
  const scripts = Object.fromEntries(await Promise.all(['sh', 'ps1'].map(async extension => [extension, await readFile(`scripts/install.${extension}`)])))
  const requests: string[] = []
  const server = createServer((request, response) => {
    const url = request.url ?? ''
    requests.push(url)
    if (url === '/api/latest') { response.setHeader('Content-Type', 'application/json'); response.end(JSON.stringify({ tag_name: 'v0.1.0' })) }
    else if (url === '/download/v0.1.0/SHA256SUMS') response.end(checksums)
    else if (url === `/download/v0.1.0/${nativeAsset}`) response.end(archives[nativeAsset])
    else if (url === `/download/v0.1.0/${webAsset}`) response.end(archives[webAsset])
    else if (url === '/scripts/service.sh') response.end(helpers.sh)
    else if (url === '/scripts/service.ps1') { response.setHeader('Content-Type', 'text/plain'); response.end(helpers.ps1) }
    else if (!options.missingScript && url === '/scripts/install.sh') response.end(scripts.sh)
    else if (!options.missingScript && url === '/scripts/install.ps1') { response.setHeader('Content-Type', 'text/plain'); response.end(scripts.ps1) }
    else { response.statusCode = 404; response.end() }
  })
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve))
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`
  return {
    directory, destination, native, web, requests,
    async run(component: string, options: { pinned?: boolean; useDefaultDestination?: boolean; service?: boolean; persistPath?: boolean } = {}) {
      const env = { ...process.env, SOLMU_NO_SERVICE: options.service ? '0' : '1', SOLMU_NO_PATH: options.persistPath ? '0' : '1', SOLMU_SERVICE_DIR: path.join(directory, 'state'), SOLMU_WEB_INSTALL_DIR: path.join(directory, 'state', 'web'), SOLMU_COMPONENT: 'all', SOLMU_VERSION: options.pinned ? '0.1.0' : '', SOLMU_INSTALLER_BASE_URL: `${base}/scripts`, SOLMU_RELEASE_API: `${base}/api`, SOLMU_RELEASE_BASE_URL: `${base}/download`, SOLMU_INSTALL_DIR: options.useDefaultDestination ? '' : destination }
      delete env.PSModulePath
      if (options.persistPath) {
        env.HOME = path.join(directory, 'home')
        env.SHELL = '/bin/bash'
        await mkdir(env.HOME, { recursive: true })
      }
      const script = path.resolve(`scripts/install-${component}.${windows ? 'ps1' : 'sh'}`)
      if (options.service) {
        env.HOME = path.join(directory, 'home')
        env.XDG_CONFIG_HOME = path.join(directory, 'home', '.config')
        env.SOLMU_SERVICE_RECORD = path.join(directory, 'service-record')
        await mkdir(env.HOME, { recursive: true })
        if (windows) {
          const harness = path.join(directory, 'service-harness.ps1')
          await writeFile(harness, `
$ErrorActionPreference = 'Stop'
function Get-ScheduledTask { param($TaskName, $ErrorAction) if (Test-Path ($env:SOLMU_SERVICE_RECORD + '.' + $TaskName + '.json')) { [pscustomobject]@{State='Stopped'} } }
function Stop-ScheduledTask { param($TaskName) Add-Content $env:SOLMU_SERVICE_RECORD 'stop' }
function New-ScheduledTaskAction { param($Execute, $Argument, $WorkingDirectory) [pscustomobject]@{Execute=$Execute; Argument=$Argument; WorkingDirectory=$WorkingDirectory} }
function New-ScheduledTaskTrigger { param([switch]$AtLogOn, [switch]$Daily, $At, $User) [pscustomobject]@{AtLogOn=$AtLogOn.IsPresent; Daily=$Daily.IsPresent; At="$At"; User=$User} }
function New-ScheduledTaskPrincipal { param($UserId, $LogonType, $RunLevel) [pscustomobject]@{UserId=$UserId; LogonType=$LogonType; RunLevel=$RunLevel} }
function New-ScheduledTaskSettingsSet { param($ExecutionTimeLimit, [switch]$AllowStartIfOnBatteries, [switch]$DontStopIfGoingOnBatteries, $MultipleInstances, $RestartCount, $RestartInterval) [pscustomobject]@{ExecutionTimeLimit=$ExecutionTimeLimit.ToString(); MultipleInstances=$MultipleInstances} }
function Register-ScheduledTask { param($TaskName, $Action, $Trigger, $Principal, $Settings, [switch]$Force) @{Name=$TaskName; Action=$Action; Trigger=$Trigger; Principal=$Principal; Settings=$Settings} | ConvertTo-Json -Depth 5 | Set-Content ($env:SOLMU_SERVICE_RECORD + '.' + $TaskName + '.json'); Add-Content $env:SOLMU_SERVICE_RECORD 'register' }
function Start-ScheduledTask { param($TaskName) Add-Content $env:SOLMU_SERVICE_RECORD 'start' }
& $args[0] -NoPath
`)
          return execute('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', harness, script], { cwd: directory, timeout: 20_000, env })
        }
        const commands = path.join(directory, 'commands')
        await mkdir(commands, { recursive: true })
        for (const command of ['systemctl', 'launchctl']) {
          const filename = path.join(commands, command)
          await writeFile(filename, '#!/bin/sh\nprintf "%s\\n" "$*" >> "$SOLMU_SERVICE_RECORD"\n')
          await chmod(filename, 0o755)
        }
        env.PATH = commands + path.delimiter + env.PATH
      }
      return windows
        ? execute('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', script, '-NoPath'], { cwd: directory, timeout: 20_000, env })
        : execute('sh', [script], { cwd: directory, timeout: 20_000, env })
    },
    async runAutoUpdate() {
      const env = { ...process.env, HOME: path.join(directory, 'home'), USERPROFILE: path.join(directory, 'home'), SOLMU_SERVICE_DIR: path.join(directory, 'state'), SOLMU_RELEASE_API: `${base}/api`, SOLMU_INSTALLER_BASE_URL: `${base}/scripts` }
      delete env.PSModulePath
      const script = path.join(directory, 'state', `auto-update.${windows ? 'ps1' : 'sh'}`)
      return windows
        ? execute('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', script], { cwd: directory, timeout: 20_000, env })
        : execute('sh', [script], { cwd: directory, timeout: 20_000, env })
    },
    async close() {
      server.closeAllConnections()
      await new Promise<void>(resolve => server.close(() => resolve()))
      await rm(directory, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 })
    },
  }
}

export function nativeInstallerTests(component: string, names: string[]) {
  test(`${component} installer installs only its component and required runtime`, async () => {
    const fixture = await releaseFixture()
    try {
      expect((await fixture.run(component)).stdout).toContain(`Installed Solmu v0.1.0 (${component})`)
      expect((await readdir(fixture.destination)).sort()).toEqual(names.map(executable).sort())
      for (const name of names.map(executable)) expect(await readFile(path.join(fixture.destination, name))).toEqual(Buffer.from(fixture.native[name]))
    } finally { await fixture.close() }
  })
  test(`${component} installer pins versions and preserves other installed programs`, async () => {
    const fixture = await releaseFixture()
    try {
      await mkdir(fixture.destination)
      await writeFile(path.join(fixture.destination, 'another-program'), 'Keep me')
      await fixture.run(component, { pinned: true })
      expect(await readFile(path.join(fixture.destination, 'another-program'), 'utf8')).toBe('Keep me')
      expect(fixture.requests).not.toContain('/api/latest')
    } finally { await fixture.close() }
  })
  test(`${component} installer rejects corrupt downloads before installing`, async () => {
    const fixture = await releaseFixture({ corrupt: true })
    try {
      await expect(fixture.run(component)).rejects.toThrow(/checksum mismatch/)
      await expect(readdir(fixture.destination)).rejects.toThrow()
    } finally { await fixture.close() }
  })
  test(`${component} installer reports a failed shared-script download`, async () => {
    const fixture = await releaseFixture({ missingScript: true })
    try {
      await expect(fixture.run(component)).rejects.toThrow()
      await expect(readdir(fixture.destination)).rejects.toThrow()
      expect(fixture.requests).not.toContain('/api/latest')
    } finally { await fixture.close() }
  })
}
