import { Bot, Play, Square, Trash2 } from 'lucide-react'
import type { Agent, AgentAction, Ingress } from '../types'
import { IngressPanel } from './IngressPanel'
import type { FormEvent } from 'react'

type AgentCardProps = {
  agent: Agent
  selected: boolean
  ingresses: Ingress[]
  port: string
  onAction: (agent: Agent, command: AgentAction) => void
  onToggleIngresses: (agent: Agent) => void
  onPortChange: (agent: Agent, port: string) => void
  onExpose: (event: FormEvent<HTMLFormElement>, agent: Agent) => void
  onRemoveIngress: (agent: Agent, ingress: Ingress) => void
}

export function AgentCard({ agent, selected, ingresses, port, onAction, onToggleIngresses, onPortChange, onExpose, onRemoveIngress }: AgentCardProps) {
  const action = agent.status === 'Stopped' ? 'start' : 'stop'
  return <article className="agent-card">
    <div className="agent-mark"><Bot size={19} /></div>
    <div className="agent-info"><div className="agent-title"><h3>{agent.name}</h3><span className={`status status-${agent.status.toLowerCase()}`}><i />{agent.status}</span></div><p>{agent.image}</p><small>{agent.id}</small></div>
    <div className="agent-actions">
      <button onClick={() => onAction(agent, action)} title={action === 'start' ? 'Start agent' : 'Stop agent'} aria-label={`${action === 'start' ? 'Start' : 'Stop'} ${agent.name}`}>{action === 'start' ? <Play size={16} /> : <Square size={15} />}</button>
      <button className="ingress-toggle" onClick={() => onToggleIngresses(agent)} aria-expanded={selected} aria-label={`Manage ingresses for ${agent.name}`}>Ports</button>
      <button className="delete-action" onClick={() => onAction(agent, 'delete')} title="Delete agent" aria-label={`Delete ${agent.name}`}><Trash2 size={16} /></button>
    </div>
    {selected && <IngressPanel agent={agent} ingresses={ingresses} port={port} onPortChange={value => onPortChange(agent, value)} onExpose={onExpose} onRemove={onRemoveIngress} />}
  </article>
}
