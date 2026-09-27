import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { request, type SkillCatalog } from '@/lib/api'

export default function Skills({ threadId, revision, onClose }: { threadId: string; revision: number; onClose: () => void }) {
  const [catalog, setCatalog] = useState<SkillCatalog | null>(null)
  const [error, setError] = useState('')
  useEffect(() => {
    let disposed = false
    void request<SkillCatalog>(`/threads/${threadId}/skills`).then(value => {
      if (!disposed) { setCatalog(value); setError('') }
    }).catch(error => { if (!disposed) setError(error instanceof Error ? error.message : 'Cannot load skills') })
    return () => { disposed = true }
  }, [threadId, revision])
  return <section className="skills-panel" aria-label="Workspace skills">
    <div className="skills-heading"><div><span className="eyebrow">FROM YOUR WORKSPACE</span><h1>Workspace skills</h1></div><Button variant="ghost" onClick={onClose}>Back to conversation</Button></div>
    <p>Discovered automatically in <code>.agents/skills/</code>. Solmu reads relevant skill instructions when working on your request.</p>
    {catalog?.directory && <p className="skills-path">{catalog.directory}</p>}
    {error && <p role="alert" className="error">{error}</p>}
    {!catalog && !error && <p role="status">Loading skills…</p>}
    {catalog?.items.length === 0 && <p>No skills installed in this workspace.</p>}
    {catalog?.items.map(skill => <article className="skill-card" key={skill.path}><h2>{skill.name}</h2><p>{skill.description}</p><code>{skill.path}</code>{skill.compatibility && <p className="skill-compatibility">Requires: {skill.compatibility}</p>}</article>)}
    {catalog?.issues.map(issue => <div className="skill-issue" key={issue.path}><strong>Not loaded: {issue.path}</strong><p>{issue.message}</p></div>)}
    <p className="skills-install">Install skill folders in <code>.agents/skills/</code>; each needs <code>SKILL.md</code>.</p>
  </section>
}
