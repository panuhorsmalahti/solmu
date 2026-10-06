import { Bot, LoaderCircle } from 'lucide-react'
import type { FormEvent } from 'react'
import type { Agent, AgentAction, Ingress } from '../types'
import { AgentCard } from './AgentCard'

type AgentListProps = {
  agents: Agent[]
  loading: boolean
  selectedAgent: string
  ingresses: Record<string, Ingress[]>
  ports: Record<string, string>
  onAction: (agent: Agent, command: AgentAction) => void
  onToggleIngresses: (agent: Agent) => void
  onPortChange: (agent: Agent, port: string) => void
  onExpose: (event: FormEvent<HTMLFormElement>, agent: Agent) => void
  onRemoveIngress: (agent: Agent, ingress: Ingress) => void
}

export function AgentList(props: AgentListProps) {
  const { agents, loading, selectedAgent, ingresses, ports, onAction, onToggleIngresses, onPortChange, onExpose, onRemoveIngress } = props

  if (loading) return <div className="loading"><LoaderCircle className="spin" />Connecting to cluster…</div>
  if (agents.length === 0) return <div className="empty"><div className="empty-icon"><Bot size={24} /></div><h3>No agents yet</h3><p>Launch your first Solmu agent. It will start in a dedicated OpenShell sandbox.</p></div>

  return <div className="agent-list">{agents.map(agent => <AgentCard
    key={agent.id}
    agent={agent}
    selected={selectedAgent === agent.id}
    ingresses={ingresses[agent.id] ?? []}
    port={ports[agent.id] ?? ''}
    onAction={onAction}
    onToggleIngresses={onToggleIngresses}
    onPortChange={onPortChange}
    onExpose={onExpose}
    onRemoveIngress={onRemoveIngress}
  />)}</div>
}
