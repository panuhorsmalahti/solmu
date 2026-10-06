import type { ReactNode } from 'react'
import { Bot } from 'lucide-react'

type AppLayoutProps = { agentCount: number; children: ReactNode }

export function AppLayout({ agentCount, children }: AppLayoutProps) {
  return <div className="shell">
    <aside className="sidebar">
      <a className="brand" href="/" aria-label="Congregator home"><span className="brand-icon"><Bot size={19} /></span>congregator</a>
      <div className="workspace"><span className="workspace-dot" />Cluster workspace</div>
      <nav><a className="nav-active" href="#agents"><Bot size={16} />Agents<span className="nav-count">{agentCount}</span></a></nav>
      <div className="side-note"><span className="side-label">RUNTIME</span><span className="runtime-name">NVIDIA OpenShell</span><span className="runtime-detail">Kubernetes sandboxes</span></div>
      <div className="sidebar-footer"><span className="live-dot" />Live cluster updates</div>
    </aside>
    <main className="main" id="agents">
      <header className="topbar"><div className="crumb">Workspace <span>/</span> Agents</div><div className="top-status"><span className="live-dot" />Connected</div></header>
      <section className="content">
        <div className="heading-row"><div><p className="eyebrow">SANDBOX CONTROL PLANE</p><h1>Your agents, gathered.</h1><p className="lede">Launch and manage Solmu agents in isolated OpenShell sandboxes.</p></div><div className="agent-count"><strong>{agentCount.toString().padStart(2, '0')}</strong><span>active agents</span></div></div>
        {children}
        <footer className="content-footer"><span>Solmu · Congregator</span><span>Sandbox operations powered by NVIDIA OpenShell</span></footer>
      </section>
    </main>
  </div>
}
