import type { FormEvent } from 'react'
import type { Agent, Ingress } from '../types'

type IngressPanelProps = {
  agent: Agent
  ingresses: Ingress[]
  port: string
  onPortChange: (port: string) => void
  onExpose: (event: FormEvent<HTMLFormElement>, agent: Agent) => void
  onRemove: (agent: Agent, ingress: Ingress) => void
}

export function IngressPanel({ agent, ingresses, port, onPortChange, onExpose, onRemove }: IngressPanelProps) {
  return <div className="ingress-panel">
    <div><strong>Exposed ports</strong><p>Each port gets a private OpenShell service URL.</p></div>
    <form onSubmit={event => onExpose(event, agent)}>
      <label><span>CONTAINER PORT</span><input inputMode="numeric" aria-label={`Port for ${agent.name}`} value={port} onChange={event => onPortChange(event.target.value)} placeholder="e.g. 8080" /></label>
      <button disabled={!port.trim()}>Expose port</button>
    </form>
    <ul>{ingresses.map(ingress => <li key={ingress.name}>
      <span>:{ingress.port}</span><a href={ingress.url} target="_blank" rel="noreferrer">{ingress.url}</a>
      {ingress.name !== 'solmu' && <button onClick={() => onRemove(agent, ingress)} aria-label={`Remove port ${ingress.port}`}>Remove</button>}
    </li>)}</ul>
  </div>
}
