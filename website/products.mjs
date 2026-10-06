import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

const products = [
  {
    slug: 'muxer', title: 'Muxer', descriptor: 'A workspace for many threads at once.',
    description: 'Bring real terminals and Solmu conversations together in named project spaces. Move between tabs, split the view into panes, and leave long-running work in place while you switch tasks.',
    image: 'screenshots/muxer-gui.png', alt: 'Muxer GUI showing a Solmu workspace and Spaces sidebar',
    eyebrow: 'LOCAL WORKSPACES · TERMINAL + NATIVE GUI',
    features: [
      ['Spaces that remember', 'Group work by project. Each space keeps its own workspace, tabs, and selected view.'],
      ['Real terminals, side by side', 'Run shells and project commands in interactive terminal panes alongside Solmu conversations.'],
      ['A window when you want one', 'Muxer GUI opens the shared Solmu desktop or a full Terminal space. It can run without the Muxer server.'],
    ],
    docs: '../docs/muxer/', repo: 'https://github.com/panuhorsmalahti/solmu/tree/main/muxer', action: 'Install Muxer',
    command: 'muxer',
  },
  {
    slug: 'boxer', title: 'Boxer', descriptor: 'Run agents with clear boundaries.',
    description: 'Launch Solmu or another command in a managed process sandbox. Start with familiar defaults, then limit workspace access and add platform-supported controls when a task needs tighter boundaries.',
    image: null, alt: '', eyebrow: 'LOCAL SANDBOX · LINUX, MACOS + WINDOWS',
    features: [
      ['Choose what files are available', 'On Linux and macOS, restrict access to a project workspace and add read-only or writable paths.'],
      ['Reuse profiles', 'Keep launch settings for Solmu, Codex, Claude Code, and other commands in named profiles.'],
      ['Inspect the work', 'Run detached sessions, inspect their status and output, and review optional workspace snapshots.'],
    ],
    docs: '../docs/boxer/', repo: 'https://github.com/panuhorsmalahti/solmu/tree/main/sandbox', action: 'Install Boxer',
    command: 'boxer --profile solmu --cwd /path/to/project -- solmu-backend',
  },
  {
    slug: 'congregator', title: 'Congregator', descriptor: 'Operate Solmu agents across a Kubernetes cluster.',
    description: 'Launch and manage Solmu agents in NVIDIA OpenShell sandboxes. Follow their status, control their lifecycle, and expose application ports through OpenShell service URLs.',
    image: 'screenshots/congregator.png', alt: 'Congregator dashboard showing an agent and exposed sandbox ports',
    eyebrow: 'CLOUD CONTROL PLANE · KUBERNETES + OPENSHELL',
    features: [
      ['Launch managed sandboxes', 'Create Solmu agents as dedicated OpenShell sandboxes and see their state update automatically.'],
      ['Expose application ports', 'Choose a port in the sandbox and get a gateway-routed service URL. Remove exposed endpoints from the dashboard.'],
      ['Operate from one dashboard', 'Inspect, start, stop, and delete the Solmu agents managed by Congregator.'],
    ],
    docs: '../docs/congregator/', repo: 'https://github.com/panuhorsmalahti/solmu/tree/main/cloud/congregator', action: 'Install Congregator',
    command: null,
  },
]

const esc = value => String(value).replace(/[&<>"']/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[ch])

function renderProduct(product) {
  const visual = product.image
    ? `<a class="product-visual product-image" href="../${product.image}"><img src="../${product.image}" alt="${esc(product.alt)}"/></a>`
    : `<div class="product-visual boxer-visual" aria-label="Boxer sandbox command example"><span class="visual-label">YOUR PROJECT</span><div class="sandbox-frame"><span class="frame-label">BOXER PROFILE · SOLMU</span><code>agent process</code><code>project workspace</code><span class="network-status">Network allowed by default</span></div><span class="visual-label">YOUR OPERATING SYSTEM</span></div>`
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width,initial-scale=1"/>
<meta name="description" content="${esc(product.description)}"/><meta name="theme-color" content="#f5f2e9"/>
<title>${esc(product.title)} · Solmu</title><link rel="icon" href="../icon.svg" type="image/svg+xml"/><link rel="stylesheet" href="../style.css"/><link rel="stylesheet" href="../product.css"/></head>
<body class="product-page product-${product.slug}"><a class="skip" href="#main">Skip to content</a>
<header class="header wrap"><a class="wordmark" href="../" aria-label="Solmu home"><img src="../icon.svg" alt="" width="26" height="26"/>solmu</a><nav aria-label="Main navigation"><a href="../#possibilities">Possibilities</a><a href="../#your-space">Clients</a><a class="docs-link" href="../docs/">Docs</a><a class="repo-link" href="https://github.com/panuhorsmalahti/solmu">GitHub <span aria-hidden="true">↗</span></a></nav></header>
<main id="main"><section class="product-hero wrap"><div class="product-copy"><p class="eyebrow">${esc(product.eyebrow)}</p><h1>${esc(product.title)}<br/><em>${esc(product.descriptor)}</em></h1><p class="product-intro">${esc(product.description)}</p><div class="product-actions"><a class="primary" href="${product.docs}">Read the guide <span aria-hidden="true">↗</span></a><a class="subtle" href="${esc(product.repo)}">View source <span aria-hidden="true">↗</span></a></div>${product.command ? `<pre><code>${esc(product.command)}</code></pre>` : ''}</div>${visual}</section>
<section class="product-details wrap"><div class="product-section-heading"><p class="eyebrow">MADE FOR THE WAY YOU WORK</p><h2>${esc(product.title)} keeps the next step close.</h2></div><div class="product-features">${product.features.map(([title, detail], index) => `<article><span>0${index + 1}</span><h2>${esc(title)}</h2><p>${esc(detail)}</p></article>`).join('')}</div><a class="subtle product-guide" href="${product.docs}">Setup, controls, and details in the ${esc(product.title)} guide ↗</a></section>
<section class="product-next wrap"><p>Part of the Solmu workspace.</p><div><a href="../muxer/">Muxer</a><a href="../boxer/">Boxer</a><a href="../congregator/">Congregator</a></div><a class="subtle" href="../">Back to Solmu home ↗</a></section></main>
<footer class="product-footer wrap"><a class="wordmark" href="../"><img src="../icon.svg" alt="" width="23" height="23"/>solmu</a><span>Open source, connected by design.</span><a href="${esc(product.repo)}">Explore ${esc(product.title)} ↗</a></footer></body></html>`
}

export async function buildProductPages(siteDirectory) {
  for (const product of products) {
    const directory = join(siteDirectory, product.slug)
    await mkdir(directory, { recursive: true })
    await writeFile(join(directory, 'index.html'), renderProduct(product))
  }
}
