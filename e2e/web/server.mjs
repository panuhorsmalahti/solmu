import { spawn } from 'node:child_process'
import { resolve } from 'node:path'
import { createInterface } from 'node:readline'

const fixture = spawn(resolve(process.env.CARGO_TARGET_DIR || 'target', 'debug', `fixture${process.platform === 'win32' ? '.exe' : ''}`), [], { stdio: ['pipe', 'pipe', 'inherit'] })
const lines = createInterface({ input: fixture.stdout })
const url = await new Promise((resolve, reject) => {
  lines.once('line', resolve)
  fixture.once('error', reject)
  fixture.once('exit', code => reject(new Error(`Fixture exited with ${code}`)))
})
const vite = spawn(process.execPath, [resolve('node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', '5175', '--strictPort'], {
  cwd: resolve('clients/web'), env: { ...process.env, SOLMU_BACKEND_URL: url }, stdio: ['ignore', 'inherit', 'inherit'],
})
function stop() { vite.kill(); fixture.stdin.end(); }
process.on('SIGTERM', () => { stop(); process.exit() })
process.on('SIGINT', () => { stop(); process.exit() })
vite.once('exit', code => { fixture.stdin.end(); process.exit(code ?? 1) })
vite.once('error', error => { console.error(error); stop(); process.exit(1) })
