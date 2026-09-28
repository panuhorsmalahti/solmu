import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { request, type McpCatalog } from '@/lib/api'

export default function Mcp({ threadId, revision, onClose }: { threadId: string; revision: number; onClose: () => void }) {
  const [catalog, setCatalog] = useState<McpCatalog | null>(null)
  const [error, setError] = useState('')
  useEffect(() => {
    let disposed = false
    void request<McpCatalog>(`/threads/${threadId}/mcp`).then(value => {
      if (!disposed) { setCatalog(value); setError('') }
    }).catch(error => { if (!disposed) setError(error instanceof Error ? error.message : 'Cannot load MCP status') })
    return () => { disposed = true }
  }, [threadId, revision])
  return <section className="skills-panel" aria-label="MCP servers">
    <div className="skills-heading"><div><span className="eyebrow">CONNECTED TO YOUR WORKSPACE</span><h1>MCP servers</h1></div><Button variant="ghost" onClick={onClose}>Back to conversation</Button></div>
    <p>Solmu connects automatically to configured servers and makes their tools available to the agent.</p>
    {catalog?.workspace && <p className="skills-path">{catalog.workspace}</p>}
    {error && <p role="alert" className="error">{error}</p>}
    {!catalog && !error && <p role="status">Connecting to MCP servers…</p>}
    {catalog?.servers.length === 0 && <p>No MCP servers configured in this workspace.</p>}
    {catalog?.servers.map(server => <article className="skill-card" key={server.name}><h2>{server.name}</h2><p>{server.status} · {server.transport} · {server.tools.length} tools</p><code>{server.source} · protocol {server.protocol_version ?? 'not connected'}</code>{server.error && <p role="status" className="error">{server.error}</p>}{server.tools.map(tool => <div key={tool.agent_name}><h3>{tool.name}</h3><p>{tool.description}</p><code>{tool.agent_name}</code></div>)}</article>)}
    {catalog?.issues.map(issue => <div className="skill-issue" key={issue.path}><strong>Not loaded: {issue.path}</strong><p>{issue.message}</p></div>)}
    <p className="skills-install">Configure servers in <code>.mcp.json</code> or <code>mcp.json</code>. Changes apply automatically.</p>
  </section>
}
