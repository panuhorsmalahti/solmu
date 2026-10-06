import { type FormEvent, useCallback, useEffect, useState } from 'react'
import { Bot, CirclePlus, LoaderCircle, Play, Square, Trash2 } from 'lucide-react'

type Agent = { id: string; name: string; status: string; image: string }
type Ingress = { name: string; port: number; url: string }

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api/v1${path}`, {
    ...init,
    headers: { 'content-type': 'application/json', ...init?.headers },
  })
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: response.statusText }))
    throw new Error(body.error || 'Request failed')
  }
  if (response.status === 204) return undefined as T
  return response.json()
}

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

  async function createAgent(event: FormEvent) {
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
    } finally { setCreating(false) }
  }

  async function action(agent: Agent, command: 'start' | 'stop' | 'delete') {
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

  async function exposePort(event: FormEvent, agent: Agent) {
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

  return <div className="shell">
    <aside className="sidebar">
      <a className="brand" href="/" aria-label="Congregator home"><span className="brand-icon"><Bot size={19} /></span>congregator</a>
      <div className="workspace"><span className="workspace-dot" />Cluster workspace</div>
      <nav><a className="nav-active" href="#agents"><Bot size={16} />Agents<span className="nav-count">{agents.length}</span></a></nav>
      <div className="side-note"><span className="side-label">RUNTIME</span><span className="runtime-name">NVIDIA OpenShell</span><span className="runtime-detail">Kubernetes sandboxes</span></div>
      <div className="sidebar-footer"><span className="live-dot" />Live cluster updates</div>
    </aside>
    <main className="main" id="agents">
      <header className="topbar"><div className="crumb">Workspace <span>/</span> Agents</div><div className="top-status"><span className="live-dot" />Connected</div></header>
      <section className="content">
        <div className="heading-row"><div><p className="eyebrow">SANDBOX CONTROL PLANE</p><h1>Your agents, gathered.</h1><p className="lede">Launch and manage Solmu agents in isolated OpenShell sandboxes.</p></div><div className="agent-count"><strong>{agents.length.toString().padStart(2, '0')}</strong><span>active agents</span></div></div>
        <form className="launch-card" onSubmit={createAgent}>
          <div className="launch-copy"><div className="launch-icon"><CirclePlus size={19} /></div><div><strong>Launch a Solmu agent</strong><p>A dedicated sandbox will be created in your cluster.</p></div></div>
          <div className="launch-fields"><label><span>AGENT NAME</span><input value={name} onChange={event => setName(event.target.value)} placeholder="e.g. docs-refresh" maxLength={80} required /></label><button className="launch-button" disabled={creating || !name.trim()}>{creating ? <LoaderCircle className="spin" size={16} /> : <CirclePlus size={16} />}{creating ? 'Launching' : 'Launch agent'}</button></div>
        </form>
        <div className="section-heading"><div><h2>Agents</h2><p>Each agent runs in its own managed sandbox. Updates appear automatically.</p></div></div>
        {error && <div className="error-banner" role="alert">{error}</div>}
        {loading ? <div className="loading"><LoaderCircle className="spin" />Connecting to cluster…</div> : agents.length === 0 ? <div className="empty"><div className="empty-icon"><Bot size={24} /></div><h3>No agents yet</h3><p>Launch your first Solmu agent. It will start in a dedicated OpenShell sandbox.</p></div> : <div className="agent-list">{agents.map(agent => <article className="agent-card" key={agent.id}>
          <div className="agent-mark"><Bot size={19} /></div>
          <div className="agent-info"><div className="agent-title"><h3>{agent.name}</h3><span className={`status status-${agent.status.toLowerCase()}`}><i />{agent.status}</span></div><p>{agent.image}</p><small>{agent.id}</small></div>
          <div className="agent-actions">{agent.status === 'Stopped' ? <button onClick={() => void action(agent, 'start')} title="Start agent" aria-label={`Start ${agent.name}`}><Play size={16} /></button> : <button onClick={() => void action(agent, 'stop')} title="Stop agent" aria-label={`Stop ${agent.name}`}><Square size={15} /></button>}<button className="ingress-toggle" onClick={() => setSelectedAgent(current => current === agent.id ? '' : agent.id)} aria-expanded={selectedAgent === agent.id} aria-label={`Manage ingresses for ${agent.name}`}>Ports</button><button className="delete-action" onClick={() => void action(agent, 'delete')} title="Delete agent" aria-label={`Delete ${agent.name}`}><Trash2 size={16} /></button></div>
          {selectedAgent === agent.id && <div className="ingress-panel"><div><strong>Exposed ports</strong><p>Each port gets a private OpenShell service URL.</p></div><form onSubmit={event => void exposePort(event, agent)}><label><span>CONTAINER PORT</span><input inputMode="numeric" aria-label={`Port for ${agent.name}`} value={ports[agent.id] ?? ''} onChange={event => setPorts(current => ({ ...current, [agent.id]: event.target.value }))} placeholder="e.g. 8080" /></label><button disabled={!ports[agent.id]?.trim()}>Expose port</button></form><ul>{(ingresses[agent.id] ?? []).map(ingress => <li key={ingress.name}><span>:{ingress.port}</span><a href={ingress.url} target="_blank" rel="noreferrer">{ingress.url}</a>{ingress.name !== 'solmu' && <button onClick={() => void removeIngress(agent, ingress)} aria-label={`Remove port ${ingress.port}`}>Remove</button>}</li>)}</ul></div>}
        </article>)}</div>}
        <footer className="content-footer"><span>Solmu · Congregator</span><span>Sandbox operations powered by NVIDIA OpenShell</span></footer>
      </section>
    </main>
  </div>
}
