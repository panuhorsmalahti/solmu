import { test, expect } from '@playwright/test'
import { createServer } from 'node:http'
import { createHash } from 'node:crypto'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { gzipSync, zipSync, strToU8 } from 'fflate'

const execute = promisify(execFile)
const windows = process.platform === 'win32'
const names = ['solmu-backend', 'solmu-cli', 'solmu-desktop', 'sandbox'].map(name => name + (windows ? '.exe' : ''))
const files = Object.fromEntries(names.map(name => [name, strToU8(`Solmu release fixture: ${name}\n`)]))

function tarball() {
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
    const checksum = header.reduce((sum, value) => sum + value, 0)
    header.write(checksum.toString(8).padStart(6, '0') + '\0 ', 148)
    parts.push(header, Buffer.from(data), Buffer.alloc((512 - data.length % 512) % 512))
  }
  return gzipSync(Buffer.concat([...parts, Buffer.alloc(1024)]))
}

for (const corrupt of [false, true]) {
  test(corrupt ? 'installer rejects a bad checksum before installing' : 'installer selects the latest release and installs every executable', async () => {
    const directory = await mkdtemp(path.join(tmpdir(), 'solmu-installer-e2e-'))
    const destination = path.join(directory, 'install with spaces')
    const platform = windows ? 'windows-x86_64' : process.platform === 'darwin' ? `macos-${process.arch === 'arm64' ? 'aarch64' : 'x86_64'}` : 'linux-x86_64'
    const asset = `solmu-v0.1.0-${platform}.${windows ? 'zip' : 'tar.gz'}`
    const archive = windows ? zipSync(files) : tarball()
    const checksum = corrupt ? '0'.repeat(64) : createHash('sha256').update(archive).digest('hex')
    const server = createServer((request, response) => {
      if (request.url === '/api/latest') { response.setHeader('Content-Type', 'application/json'); response.end(JSON.stringify({ tag_name: 'v0.1.0' })) }
      else if (request.url === `/download/v0.1.0/${asset}`) response.end(archive)
      else if (request.url === '/download/v0.1.0/SHA256SUMS') response.end(`${checksum}  ${asset}\n`)
      else { response.statusCode = 404; response.end() }
    })
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve))
    const address = server.address() as { port: number }
    const base = `http://127.0.0.1:${address.port}`
    try {
      // PowerShell 7 CI must not pass its incompatible module paths to 5.1.
      const installerEnvironment = { ...process.env }
      delete installerEnvironment.PSModulePath
      const installation = windows
        ? execute('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.resolve('scripts/install.ps1'), '-InstallDir', destination, '-ReleaseApi', `${base}/api`, '-DownloadBase', `${base}/download`, '-NoPath'], { timeout: 20_000, env: installerEnvironment })
        : execute('sh', [path.resolve('scripts/install.sh')], { timeout: 20_000, env: { ...process.env, SOLMU_VERSION: '', SOLMU_INSTALL_DIR: destination, SOLMU_RELEASE_API: `${base}/api`, SOLMU_RELEASE_BASE_URL: `${base}/download` } })
      if (corrupt) {
        await expect(installation).rejects.toThrow(/checksum mismatch/)
        await expect(readdir(destination)).rejects.toThrow()
      } else {
        expect((await installation).stdout).toContain('Installed Solmu v0.1.0')
        expect((await readdir(destination)).sort()).toEqual([...names].sort())
        for (const name of names) expect(await readFile(path.join(destination, name))).toEqual(Buffer.from(files[name]))
      }
    } finally {
      server.closeAllConnections()
      await new Promise<void>(resolve => server.close(() => resolve()))
      await rm(directory, { recursive: true, force: true })
    }
  })
}
