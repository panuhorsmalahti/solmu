import { expect, test } from '@playwright/test'
import { execFile, spawn } from 'node:child_process'
import { promisify } from 'node:util'
import { readFile, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { releaseFixture } from '../install/support'

test('Windows background supervisor contains its child and writes logs while running', async () => {
  test.skip(process.platform !== 'win32', 'Windows Job Object supervisor')
  const fixture = await releaseFixture()
  let supervisor: ReturnType<typeof spawn> | undefined
  let childPid: number | undefined
  try {
    await fixture.run('backend', { service: true })
    const state = path.join(fixture.directory, 'state')
    const child = path.join(fixture.directory, 'service-child.exe')
    const compile = path.join(fixture.directory, 'compile.ps1')
    await writeFile(compile, `
Add-Type -OutputAssembly $args[0] -OutputType ConsoleApplication -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Threading;
public class Child {
    public static void Main() {
        Console.WriteLine(Process.GetCurrentProcess().Id);
        Console.Error.WriteLine("Solmu service test error output");
        Thread.Sleep(Timeout.Infinite);
    }
}
'@
`)
    const environment = { ...process.env }
    delete environment.PSModulePath
    await promisify(execFile)('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', compile, child], { env: environment })
    supervisor = spawn('powershell.exe', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(state, 'backend-service.ps1'), '-Binary', child, '-Directory', state], { env: environment, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] })
    let errors = ''
    supervisor.stderr!.on('data', chunk => { errors += chunk })
    await expect.poll(async () => {
      if (supervisor!.exitCode !== null) throw new Error(`Supervisor exited: ${errors}`)
      return readFile(path.join(state, 'backend.log'), 'utf8').catch(() => '')
    }, { timeout: 15_000 }).toMatch(/^\d+\r?\n/)
    childPid = Number((await readFile(path.join(state, 'backend.log'), 'utf8')).trim())
    expect(() => process.kill(childPid!, 0)).not.toThrow()
    await expect.poll(() => readFile(path.join(state, 'backend-error.log'), 'utf8').catch(() => '')).toContain('Solmu service test error output')
    await new Promise<void>(resolve => { supervisor!.once('exit', () => resolve()); supervisor!.kill() })
    await expect.poll(() => { try { process.kill(childPid!, 0); return true } catch { return false } }).toBe(false)
  } finally {
    if (supervisor && supervisor.exitCode === null && supervisor.signalCode === null) await new Promise<void>(resolve => { supervisor!.once('exit', () => resolve()); supervisor!.kill() })
    if (childPid) { try { process.kill(childPid) } catch { /* Already contained and stopped. */ } }
    await fixture.close()
  }
})
