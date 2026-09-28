import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { request, type PluginCatalog } from '@/lib/api'

export default function Plugins({ threadId, revision, onClose }: { threadId: string; revision: number; onClose: () => void }) {
  const [catalog, setCatalog] = useState<PluginCatalog | null>(null)
  const [error, setError] = useState('')
  useEffect(() => {
    let disposed = false
    void request<PluginCatalog>(`/threads/${threadId}/plugins`).then(value => {
      if (!disposed) { setCatalog(value); setError('') }
    }).catch(error => { if (!disposed) setError(error instanceof Error ? error.message : 'Cannot load plugins') })
    return () => { disposed = true }
  }, [threadId, revision])
  return <section className="skills-panel" aria-label="Workspace plugins">
    <div className="skills-heading"><div><span className="eyebrow">FROM YOUR WORKSPACE</span><h1>Plugins</h1></div><Button variant="ghost" onClick={onClose}>Back to conversation</Button></div>
    <p>Solmu discovers installed plugins automatically. They can add skills and MCP tools.</p>
    {catalog?.directory && <p className="skills-path">{catalog.directory}</p>}
    {error && <p role="alert" className="error">{error}</p>}
    {!catalog && !error && <p role="status">Loading plugins…</p>}
    {catalog?.items.length === 0 && <p>No plugins installed in this workspace.</p>}
    {catalog?.items.map(plugin => <article className="skill-card" key={plugin.path}><h2>{plugin.name}</h2>{plugin.version && <p>Version {plugin.version}</p>}{plugin.description && <p>{plugin.description}</p>}<code>{plugin.path}</code><p>{plugin.skills.length} {plugin.skills.length === 1 ? 'skill' : 'skills'} · {plugin.mcp_servers.length} MCP {plugin.mcp_servers.length === 1 ? 'server' : 'servers'}</p>{plugin.issues.map(issue => <div className="skill-issue" key={issue.path}><strong>Not loaded: {issue.path}</strong><p>{issue.message}</p></div>)}</article>)}
    {catalog?.issues.map(issue => <div className="skill-issue" key={issue.path}><strong>Not loaded: {issue.path}</strong><p>{issue.message}</p></div>)}
    <p className="skills-install">Install plugin folders in <code>.agents/plugins/</code>; each needs <code>plugin.json</code>.</p>
  </section>
}
