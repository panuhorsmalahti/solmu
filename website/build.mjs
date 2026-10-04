import { cp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import { dirname, join, posix, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { Marked } from 'marked'
import GithubSlugger from 'github-slugger'
import sanitizeHtml from 'sanitize-html'

export const repository = resolve(dirname(fileURLToPath(import.meta.url)), '..')
export const siteDirectory = join(repository, 'website', 'dist')
const sourceUrl = 'https://github.com/panuhorsmalahti/solmu/blob/main/'

const groups = [
  ['Getting started', ['index', 'bundle', 'services', 'configuration', 'running', 'releases']],
  ['Clients', ['clients', 'cli', 'desktop', 'android', 'ios', 'web', 'muxer', 'muxer-gui']],
  ['Agent', ['agent', 'profile', 'audit', 'tasks', 'workspaces', 'tools', 'skills', 'mcp', 'plugins', 'boxer']],
  ['Muxer', ['muxer-commands', 'muxer-configuration', 'muxer-automation', 'muxer-agents', 'muxer-terminals']],
  ['Reference', ['api', 'development', 'license']],
]

const escape = (text) => String(text).replace(/[&<>"']/g, (character) => ({
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
})[character])
const encodePath = (path) => path.split('/').map(encodeURIComponent).join('/')
const pagePath = (file) => file === 'index.md' ? 'docs/' : `docs/${file.slice(0, -3)}/`
const relative = (from, to) => {
  const path = posix.relative(from.replace(/\/$/, ''), to.replace(/\/$/, '')) || '.'
  return encodePath(path) + (to.endsWith('/') ? '/' : '')
}

async function markdownFiles(directory, prefix = '') {
  const entries = await readdir(directory, { withFileTypes: true })
  const files = await Promise.all(entries.sort((a, b) => a.name.localeCompare(b.name)).map(async (entry) => {
    const name = posix.join(prefix, entry.name)
    if (entry.isDirectory()) return markdownFiles(join(directory, entry.name), name)
    return entry.isFile() && entry.name.endsWith('.md') ? [name] : []
  }))
  return files.flat()
}

function rewriteLink(href, file, documents) {
  // External links and links within the same page retain their original meaning.
  if (/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(href)) return href
  const match = href.match(/^([^?#]*)([?#].*)?$/)
  let path
  try { path = decodeURIComponent(match[1]) } catch { return href }
  const target = path.startsWith('/') ? posix.normalize(path.slice(1)) : posix.normalize(posix.join('docs', posix.dirname(file), path))
  const suffix = match[2] ?? ''
  const document = documents.get(target.replace(/^docs\//, ''))
  if (target.startsWith('docs/') && document) return relative(pagePath(file), pagePath(document.file)) + suffix
  if (target.startsWith('docs/screenshots/')) return relative(pagePath(file), target.replace(/^docs\//, '')) + suffix
  // READMEs, source files, and other repository references remain available on GitHub.
  return sourceUrl + encodePath(target) + suffix
}

export function renderMarkdown(markdown, file, documents) {
  const slugger = new GithubSlugger()
  const headings = []
  const renderer = new Marked({
    gfm: true,
    renderer: {
      heading({ tokens, depth }) {
        const html = this.parser.parseInline(tokens)
        const text = sanitizeHtml(html, { allowedTags: [], allowedAttributes: {} })
        const id = slugger.slug(text)
        headings.push({ depth, text, id })
        return `<h${depth} id="${escape(id)}">${html}</h${depth}>\n`
      },
    },
    walkTokens(token) {
      if (token.type === 'link' || token.type === 'image') token.href = rewriteLink(token.href, file, documents)
    },
  })
  const html = sanitizeHtml(renderer.parse(markdown), {
    allowedTags: [...sanitizeHtml.defaults.allowedTags, 'img', 'details', 'summary', 'input'],
    allowedAttributes: {
      '*': ['id'], a: ['href', 'title'], img: ['src', 'alt', 'title', 'width', 'height'],
      code: ['class'], ol: ['start'], input: ['type', 'checked', 'disabled'],
    },
    allowedSchemes: ['http', 'https', 'mailto'],
    transformTags: { input: (_tag, attributes) => ({ tagName: 'input', attribs: { type: 'checkbox', disabled: '', ...('checked' in attributes ? { checked: '' } : {}) } }) },
  })
  return { html, headings }
}

function navigation(documents, current) {
  const included = new Set(groups.flatMap(([, files]) => files.map((file) => `${file}.md`)))
  const other = [...documents.keys()].filter((file) => !included.has(file))
  return [...groups, ['More guides', other.map((file) => file.slice(0, -3))]]
    .map(([name, files]) => {
      const pages = files.map((file) => documents.get(`${file}.md`)).filter(Boolean)
      if (!pages.length) return ''
      return `<section class="guide-group"><h2>${escape(name)}</h2><ul>${pages.map((page) =>
        `<li><a href="${relative(pagePath(current.file), pagePath(page.file))}"${page.file === current.file ? ' aria-current="page"' : ''}>${escape(page.file === 'index.md' ? 'Overview' : page.title)}</a></li>`
      ).join('')}</ul></section>`
    }).join('')
}

function documentPage(document, documents) {
  const { html, headings } = renderMarkdown(document.markdown, document.file, documents)
  const home = relative(pagePath(document.file), './')
  const assets = (name) => relative(pagePath(document.file), name)
  const toc = headings.filter(({ depth }) => depth === 2 || depth === 3)
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8"/><meta name="viewport" content="width=device-width,initial-scale=1"/>
  <meta name="description" content="${escape(document.title)} — Solmu documentation."/>
  <meta name="theme-color" content="#f5f2e9"/>
  <title>${escape(document.title)} · Solmu docs</title>
  <link rel="icon" href="${assets('icon.svg')}" type="image/svg+xml"/>
  <link rel="stylesheet" href="${assets('style.css')}"/>
  <link rel="stylesheet" href="${assets('docs.css')}"/>
  <script src="${assets('docs.js')}" defer></script>
</head>
<body class="documentation">
  <a class="skip" href="#main">Skip to content</a>
  <header class="header docs-header">
    <a class="wordmark" href="${home}" aria-label="Solmu home"><img src="${assets('icon.svg')}" alt="" width="26" height="26"/>solmu</a>
    <nav aria-label="Main navigation"><a class="docs-link" href="${relative(pagePath(document.file), 'docs/')}" aria-current="location">Docs</a><a class="repo-link" href="https://github.com/panuhorsmalahti/solmu">GitHub <span aria-hidden="true">↗</span></a></nav>
  </header>
  <div class="docs-layout">
    <aside class="guide-sidebar">
      <details class="guide-menu" open><summary>Browse guides</summary>
        <label class="guide-filter-label" for="guide-filter">Find a guide</label>
        <input id="guide-filter" type="search" placeholder="Filter guides…" autocomplete="off"/>
        <nav aria-label="Documentation guides">${navigation(documents, document)}</nav>
        <p id="no-guides" role="status" hidden>No matching guides.</p>
      </details>
    </aside>
    <main id="main" class="doc-content" tabindex="-1">
      <div class="doc-breadcrumb"><a href="${relative(pagePath(document.file), 'docs/')}">Documentation</a><span aria-hidden="true">/</span><span>${escape(document.file === 'index.md' ? 'Overview' : document.title)}</span></div>
      <article>${html}</article>
      <footer class="doc-footer"><a href="${sourceUrl}docs/${encodePath(document.file)}">View this page on GitHub ↗</a><a href="${home}">Solmu home</a></footer>
    </main>
    <aside class="doc-toc">${toc.length ? `<nav aria-label="On this page"><h2>On this page</h2><ul>${toc.map(({ depth, text, id }) => `<li class="toc-depth-${depth}"><a href="#${escape(id)}">${text}</a></li>`).join('')}</ul></nav>` : ''}</aside>
  </div>
</body>
</html>
`
}

export async function buildSite() {
  const docsDirectory = join(repository, 'docs')
  const documents = new Map(await Promise.all((await markdownFiles(docsDirectory)).map(async (file) => {
    const markdown = await readFile(join(docsDirectory, file), 'utf8')
    const heading = markdown.match(/^#\s+(.+)$/m)
    const title = heading?.[1].trim() ?? posix.basename(file, '.md').replace(/[-_]/g, ' ')
    return [file, { file, title, markdown }]
  })))
  if (!documents.has('index.md')) throw new Error('docs/index.md is required for the documentation home')
  // This fixed output directory is inside website/, separate from every source.
  await rm(siteDirectory, { recursive: true, force: true })
  await mkdir(siteDirectory, { recursive: true })
  for (const asset of ['index.html', 'style.css', 'icon.svg', 'docs.css', 'docs.js']) {
    await cp(join(repository, 'website', asset), join(siteDirectory, asset))
  }
  await cp(join(docsDirectory, 'screenshots'), join(siteDirectory, 'screenshots'), { recursive: true })
  for (const document of documents.values()) {
    const directory = join(siteDirectory, pagePath(document.file))
    await mkdir(directory, { recursive: true })
    await writeFile(join(directory, 'index.html'), documentPage(document, documents))
  }
  return documents.size
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(`Built the Solmu website and ${await buildSite()} documentation pages in website/dist`)
}
