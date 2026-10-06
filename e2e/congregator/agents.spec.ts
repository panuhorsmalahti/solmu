import { test, expect } from '@playwright/test'
import { mkdir } from 'node:fs/promises'

const readyAgent = { id: 'solmu-docs-refresh-0123abcd', name: 'docs-refresh', status: 'Running', image: 'ghcr.io/panuhorsmalahti/solmu:latest' }

test('launch, stop, start, and delete a Solmu sandbox', async ({ page }) => {
  let agents: typeof readyAgent[] = []
  const operations: string[] = []
  await page.route('**/api/v1/agents**', async route => {
    const request = route.request()
    const path = new URL(request.url()).pathname
    if (request.method() === 'GET' && path === '/api/v1/agents') return route.fulfill({ json: agents })
    if (request.method() === 'POST' && path === '/api/v1/agents') {
      const body = request.postDataJSON()
      operations.push(`create:${body.name}`)
      agents = [{ ...readyAgent, name: body.name }]
      return route.fulfill({ status: 201, json: agents[0] })
    }
    if (request.method() === 'POST' && path.endsWith('/stop')) {
      operations.push('stop')
      agents = agents.map(agent => ({ ...agent, status: 'Stopped' }))
      return route.fulfill({ json: agents[0] })
    }
    if (request.method() === 'POST' && path.endsWith('/start')) {
      operations.push('start')
      agents = agents.map(agent => ({ ...agent, status: 'Running' }))
      return route.fulfill({ json: agents[0] })
    }
    if (request.method() === 'DELETE') {
      operations.push('delete')
      agents = []
      return route.fulfill({ status: 204, body: '' })
    }
    return route.fulfill({ status: 405 })
  })
  page.on('dialog', dialog => dialog.accept())
  await page.goto('/')
  await expect(page.getByText('No agents yet')).toBeVisible()
  await page.getByLabel('AGENT NAME').fill('docs-refresh')
  await page.getByRole('button', { name: 'Launch agent' }).click()
  await expect(page.getByRole('heading', { name: 'docs-refresh' })).toBeVisible()
  await expect(page.getByText('Running')).toBeVisible()
  await page.getByRole('button', { name: 'Stop docs-refresh' }).click()
  await expect(page.getByText('Stopped')).toBeVisible()
  await page.getByRole('button', { name: 'Start docs-refresh' }).click()
  await expect(page.getByText('Running')).toBeVisible()
  await page.getByRole('button', { name: 'Delete docs-refresh' }).click()
  await expect(page.getByText('No agents yet')).toBeVisible()
  expect(operations).toEqual(['create:docs-refresh', 'stop', 'start', 'delete'])
})

test('expose and remove an agent port with its gateway ingress URL', async ({ page }) => {
  const agent = { ...readyAgent }
  let exposed = [{ name: 'solmu', port: 3000, url: 'https://solmu.example.openshell.test' }]
  const operations: string[] = []
  await page.route('**/api/v1/agents**', async route => {
    const request = route.request()
    const path = new URL(request.url()).pathname
    if (request.method() === 'GET' && path === '/api/v1/agents') return route.fulfill({ json: [agent] })
    if (request.method() === 'GET' && path.endsWith('/ingresses')) return route.fulfill({ json: exposed })
    if (request.method() === 'POST' && path.endsWith('/ingresses')) {
      const { port } = request.postDataJSON()
      const ingress = { name: `port-${port}`, port, url: `https://port-${port}.example.openshell.test` }
      exposed = [...exposed.filter(item => item.port !== port), ingress]
      operations.push(`expose:${port}`)
      return route.fulfill({ status: 201, json: ingress })
    }
    if (request.method() === 'DELETE' && path.includes('/ingresses/')) {
      const service = decodeURIComponent(path.split('/').at(-1)!)
      exposed = exposed.filter(item => item.name !== service)
      operations.push(`unexpose:${service}`)
      return route.fulfill({ status: 204, body: '' })
    }
    return route.fulfill({ status: 405 })
  })
  await page.goto('/')
  await page.getByRole('button', { name: 'Manage ingresses for docs-refresh' }).click()
  await expect(page.getByRole('link', { name: 'https://solmu.example.openshell.test' })).toBeVisible()
  await page.getByLabel('Port for docs-refresh').fill('8080')
  await page.getByRole('button', { name: 'Expose port' }).click()
  const ingressUrl = 'https://port-8080.example.openshell.test'
  await expect(page.getByRole('link', { name: ingressUrl })).toHaveAttribute('target', '_blank')

  if (process.env.SOLMU_CAPTURE_SCREENSHOTS) {
    await mkdir('docs/screenshots', { recursive: true })
    await page.setViewportSize({ width: 1440, height: 960 })
    await page.screenshot({ path: 'docs/screenshots/congregator.png', fullPage: true })
  }

  await page.getByRole('button', { name: 'Remove port 8080' }).click()
  await expect(page.getByRole('link', { name: ingressUrl })).toHaveCount(0)
  expect(operations).toEqual(['expose:8080', 'unexpose:port-8080'])
})

test('agent list reconnects and updates automatically', async ({ page }) => {
  let agents = [readyAgent]
  await page.route('**/api/v1/agents', route => route.fulfill({ json: agents }))
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'docs-refresh' })).toBeVisible()
  agents = [readyAgent, { ...readyAgent, id: 'solmu-tests-9999eeee', name: 'tests' }]
  await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')))
  await expect(page.getByRole('heading', { name: 'tests' })).toBeVisible()
})
