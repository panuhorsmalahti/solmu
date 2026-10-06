import { type FormEvent, useCallback, useEffect, useState } from 'react'
import { api } from './api'
import { AgentList } from './components/AgentList'
import { AppLayout } from './components/AppLayout'
import { ErrorBanner } from './components/ErrorBanner'
import { LaunchAgentForm } from './components/LaunchAgentForm'
import type { Agent, AgentAction, Ingress } from './types'

export default function App() {
  const [agents, setAgents] = useState<Agent[]>([])
  const [name, setName] = useState('')
  const [loading, setLoading] = useState(true)
  const [creating, setCreating] = useState(false)
  const [error, setError] = useState('')
  const [selectedAgent, setSelectedAgent] = useState('')
  const [ingresses, setIngresses] = useState<Record<string, Ingress[]>>({})
  const [ports, setPorts] = useState<Record<string, string>>({})

  const refresh = useCallback(async () => {
    try {
      const updated = await api<Agent[]>('/agents')
      setAgents(updated)
      if (selectedAgent && updated.some(agent => agent.id === selectedAgent)) {
        const exposed = await api<Ingress[]>(`/agents/${encodeURIComponent(selectedAgent)}/ingresses`)
        setIngresses(current => ({ ...current, [selectedAgent]: exposed }))
      }
      setError('')
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not connect to Congregator')
    } finally {
      setLoading(false)
    }
  }, [selectedAgent])

  useEffect(() => {
    void refresh()
    const timer = window.setInterval(() => { if (!document.hidden) void refresh() }, 5000)
    const onVisible = () => { if (!document.hidden) void refresh() }
    document.addEventListener('visibilitychange', onVisible)
    return () => { window.clearInterval(timer); document.removeEventListener('visibilitychange', onVisible) }
  }, [refresh])

  async function createAgent(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!name.trim()) return
    setCreating(true)
    setError('')
    try {
      await api<Agent>('/agents', { method: 'POST', body: JSON.stringify({ name: name.trim() }) })
      setName('')
      await refresh()
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not create the agent')
    } finally {
      setCreating(false)
    }
  }

  async function handleAgentAction(agent: Agent, command: AgentAction) {
    if (command === 'delete' && !window.confirm(`Delete ${agent.name} and its sandbox?`)) return
    setError('')
    try {
      await api(`/agents/${encodeURIComponent(agent.id)}${command === 'delete' ? '' : `/${command}`}`, { method: command === 'delete' ? 'DELETE' : 'POST' })
      if (command === 'delete' && selectedAgent === agent.id) setSelectedAgent('')
      await refresh()
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : `Could not ${command} the agent`)
    }
  }

  function toggleIngresses(agent: Agent) {
    setSelectedAgent(current => current === agent.id ? '' : agent.id)
  }

  async function exposePort(event: FormEvent<HTMLFormElement>, agent: Agent) {
    event.preventDefault()
    const port = Number(ports[agent.id])
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      setError('Enter a port between 1 and 65535')
      return
    }
    setError('')
    try {
      await api<Ingress>(`/agents/${encodeURIComponent(agent.id)}/ingresses`, { method: 'POST', body: JSON.stringify({ port }) })
      setPorts(current => ({ ...current, [agent.id]: '' }))
      const exposed = await api<Ingress[]>(`/agents/${encodeURIComponent(agent.id)}/ingresses`)
      setIngresses(current => ({ ...current, [agent.id]: exposed }))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not expose this port')
    }
  }

  async function removeIngress(agent: Agent, ingress: Ingress) {
    try {
      await api(`/agents/${encodeURIComponent(agent.id)}/ingresses/${encodeURIComponent(ingress.name)}`, { method: 'DELETE' })
      setIngresses(current => ({ ...current, [agent.id]: current[agent.id]?.filter(item => item.name !== ingress.name) ?? [] }))
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Could not remove this ingress')
    }
  }

  return <AppLayout agentCount={agents.length}>
    <LaunchAgentForm name={name} creating={creating} onNameChange={setName} onSubmit={createAgent} />
    <div className="section-heading"><div><h2>Agents</h2><p>Each agent runs in its own managed sandbox. Updates appear automatically.</p></div></div>
    <ErrorBanner message={error} />
    <AgentList
      agents={agents}
      loading={loading}
      selectedAgent={selectedAgent}
      ingresses={ingresses}
      ports={ports}
      onAction={handleAgentAction}
      onToggleIngresses={toggleIngresses}
      onPortChange={(agent, port) => setPorts(current => ({ ...current, [agent.id]: port }))}
      onExpose={exposePort}
      onRemoveIngress={removeIngress}
    />
  </AppLayout>
}
