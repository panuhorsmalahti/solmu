import { spawn } from 'node:child_process'
import { resolve } from 'node:path'
import { createInterface } from 'node:readline'

const fixture = spawn(resolve(process.env.CARGO_TARGET_DIR || 'target', 'debug', `fixture${process.platform === 'win32' ? '.exe' : ''}`), [], { env: { ...process.env, SOLMU_FIXTURE_WEB: '1', SOLMU_FIXTURE_BIND_ADDR: '127.0.0.1:5175' }, stdio: ['pipe', 'pipe', 'inherit'] })
const lines = createInterface({ input: fixture.stdout })
const url = await new Promise((resolve, reject) => {
  lines.once('line', resolve)
  fixture.once('error', reject)
  fixture.once('exit', code => reject(new Error(`Fixture exited with ${code}`)))
})
if (url !== 'http://127.0.0.1:5175') throw new Error(`Unexpected fixture URL: ${url}`)
function stop() { fixture.stdin.end(); }
process.on('SIGTERM', () => { stop(); process.exit() })
process.on('SIGINT', () => { stop(); process.exit() })
fixture.once('exit', code => process.exit(code ?? 1))
