import { createServer } from 'node:http'
import { readFile } from 'node:fs/promises'
import { resolve, sep, extname } from 'node:path'

const root = resolve('website')
createServer(async (request, response) => {
  try {
    const url = new URL(request.url, 'http://localhost')
    const pathname = decodeURIComponent(url.pathname)
    const assetRoot = pathname.startsWith('/screenshots/') ? resolve('docs/screenshots') : root
    const relative = assetRoot === root ? (pathname === '/' ? '/index.html' : pathname) : pathname.slice('/screenshots'.length)
    const path = resolve(assetRoot, `.${relative}`)
    if (!path.startsWith(assetRoot + sep)) { response.writeHead(403).end(); return }
    const mime = { '.html':'text/html', '.css':'text/css', '.js':'text/javascript', '.svg':'image/svg+xml', '.png':'image/png' }
    response.setHeader('Content-Type', mime[extname(path)] ?? 'application/octet-stream')
    response.end(await readFile(path))
  } catch { response.writeHead(404).end('Not found') }
}).listen(4174, '127.0.0.1', () => console.log('Solmu website: http://127.0.0.1:4174'))
