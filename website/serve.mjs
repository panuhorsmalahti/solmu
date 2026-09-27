import { createServer } from 'node:http'
import { readFile, stat } from 'node:fs/promises'
import { resolve, sep, extname } from 'node:path'
import { buildSite, siteDirectory } from './build.mjs'

const root = siteDirectory
const base = process.argv[2] === '--base' ? process.argv[3] : '/'
if (!base?.startsWith('/') || !base.endsWith('/') || base.includes('..')) throw new Error('Use --base /path/')
await buildSite()
createServer(async (request, response) => {
  try {
    const url = new URL(request.url, 'http://localhost')
    const pathname = decodeURIComponent(url.pathname)
    if (!pathname.startsWith(base)) { response.writeHead(404).end('Not found'); return }
    let path = resolve(root, pathname.slice(base.length) || '.')
    if (path !== root && !path.startsWith(root + sep)) { response.writeHead(403).end(); return }
    if ((await stat(path)).isDirectory()) {
      if (!pathname.endsWith('/')) {
        response.writeHead(301, { Location: url.pathname + '/' + url.search }).end()
        return
      }
      path = resolve(path, 'index.html')
    }
    const mime = { '.html':'text/html', '.css':'text/css', '.js':'text/javascript', '.svg':'image/svg+xml', '.png':'image/png' }
    response.setHeader('Content-Type', mime[extname(path)] ?? 'application/octet-stream')
    response.end(await readFile(path))
  } catch { response.writeHead(404).end('Not found') }
}).listen(4174, '127.0.0.1', () => console.log(`Solmu website: http://127.0.0.1:4174${base}`))
