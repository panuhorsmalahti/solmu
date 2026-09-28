import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { useLocation, useNavigate, useParams } from 'react-router-dom'
import ProfilePage from './Profile'
import Skills from './Skills'
import Mcp from './Mcp'
import Plugins from './Plugins'
import { ArrowUp, MessageSquare, Plus, Square, Trash2, Check, Sprout } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { list, reply, request, type Thread, type Message, type ModelCatalog, type ToolRun } from '@/lib/api'

export default function App() {
  const [skillsOpen, setSkillsOpen] = useState(false), [skillsRevision, setSkillsRevision] = useState(0)
  const [mcpOpen, setMcpOpen] = useState(false), [mcpRevision, setMcpRevision] = useState(0)
  const [pluginsOpen, setPluginsOpen] = useState(false), [pluginsRevision, setPluginsRevision] = useState(0)
  const { threadId } = useParams()
  const navigate = useNavigate()
  const isProfile = useLocation().pathname === '/profile'
  const [profileRevision, setProfileRevision] = useState(0)
  const [catalog, setCatalog] = useState<ModelCatalog | null>(null)
  const [modelPicker, setModelPicker] = useState(false), [customModel, setCustomModel] = useState('')
  const [threads, setThreads] = useState<Thread[]>([])
  const [current, setCurrent] = useState<Thread | null>(null)
  const [messages, setMessages] = useState<Message[]>([])
  const [tools, setTools] = useState<ToolRun[]>([])
  const [draft, setDraft] = useState(''), [title, setTitle] = useState('')
  const [partial, setPartial] = useState(''), [error, setError] = useState('')
  const [busy, setBusy] = useState(true)
  const [responding, setResponding] = useState(false), [connected, setConnected] = useState(true)
  const end = useRef<HTMLDivElement>(null)
  const controller = useRef<AbortController | null>(null)
  const initial = useRef(false), dirty = useRef(false)
  const loadedThread = useRef<string | null>(null), revision = useRef(0)

  async function refresh() {
    const requestedRevision = ++revision.current
    const next = await list<Thread>('/threads')
    if (requestedRevision !== revision.current) { dirty.current = true; return }
    if (current) {
      const thread = next.find(thread => thread.id === current.id)
      if (thread) {
        const history = await list<Message>(`/threads/${current.id}/messages`)
        const activity = await list<ToolRun>(`/threads/${current.id}/tools`)
        if (requestedRevision !== revision.current) { dirty.current = true; return }
        setTitle(value => value === current.title ? thread.title : value); setCurrent(thread)
        setMessages(history)
        setTools(activity)
      } else { loadedThread.current = null; setCurrent(null); setTitle(''); setMessages([]); if (!isProfile) navigate('/', { replace: true }) }
    }
    setThreads(next)
  }

  useEffect(() => {
    if (isProfile) {
      void list<Thread>('/threads').then(setThreads).catch(error => setError(describe(error))).finally(() => setBusy(false))
      return
    }
    if (threadId && loadedThread.current === threadId) return
    ++revision.current
    setBusy(true)
    let disposed = false
    void (async () => {
      try {
        let thread: Thread | null = null
        if (threadId) thread = await request<Thread>(`/threads/${threadId}`)
        else if (!initial.current) { initial.current = true; thread = await request<Thread>('/threads', 'POST', {}) }
        const history: Message[] = thread ? await list(`/threads/${thread.id}/messages`) : []
        const activity: ToolRun[] = thread ? await list(`/threads/${thread.id}/tools`) : []
        const next: Thread[] = await list('/threads')
        if (disposed) return
        initial.current = true
        loadedThread.current = thread?.id ?? null
        setCurrent(thread); setTitle(thread?.title ?? ''); setMessages(history); setThreads(next); setDraft('')
        setTools(activity)
        if (thread && !threadId) navigate(`/threads/${thread.id}`, { replace: true })
      } catch (error) { setError(describe(error)) }
      finally { if (!disposed) setBusy(false) }
    })()
    return () => {
      disposed = true
      if (controller.current) { controller.current.abort(); if (threadId) void request(`/threads/${threadId}/stop`, 'POST').catch(() => {}) }
    }
  }, [threadId, navigate, isProfile])

  useEffect(() => { void request<ModelCatalog>('/models').then(setCatalog).catch(error => setError(describe(error))) }, [profileRevision])

  const liveChange = useEffectEvent(() => {
    if (busy) { dirty.current = true; return }
    void refresh().catch(error => setError(describe(error)))
  })
  useEffect(() => {
    let socket: WebSocket | undefined, timer: ReturnType<typeof setTimeout> | undefined, disposed = false
    function connect() {
      socket = new WebSocket(`${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/api/v1/events`)
      socket.onopen = () => { setConnected(true); setProfileRevision(value => value + 1); setSkillsRevision(value => value + 1); setMcpRevision(value => value + 1); setPluginsRevision(value => value + 1); liveChange() }
      socket.onmessage = event => { const type = JSON.parse(event.data).type; if (type === 'conversation_changed') liveChange(); if (type === 'profile_changed') setProfileRevision(value => value + 1); if (type === 'skills_changed') setSkillsRevision(value => value + 1); if (type === 'mcp_changed') setMcpRevision(value => value + 1); if (type === 'plugins_changed') setPluginsRevision(value => value + 1) }
      socket.onclose = () => { if (!disposed) { setConnected(false); timer = setTimeout(connect, 2000) } }
      socket.onerror = () => socket?.close()
    }
    connect()
    return () => { disposed = true; clearTimeout(timer); socket?.close(); controller.current?.abort() }
  }, [])
  useEffect(() => { if (!busy && dirty.current) { dirty.current = false; liveChange() } }, [busy])

  useEffect(() => { end.current?.scrollIntoView({ block: 'end' }) }, [messages, partial, tools])

  async function act(operation: () => Promise<void>) {
    if (busy) return
    ++revision.current
    setBusy(true); setError(''); setPartial('')
    try { await operation() }
    catch (error) { if (!(error instanceof Error && (error.name === 'AbortError' || error.message === 'Response stopped'))) setError(describe(error)); setPartial('') }
    finally { setBusy(false) }
  }

  function newThread() {
    if (current && messages.length === 0) return
    void act(async () => {
      const thread = await request<Thread>('/threads', 'POST', {})
      loadedThread.current = thread.id
      setTools([])
      setCurrent(thread); setTitle(thread.title); setMessages([]); setDraft(''); setThreads(await list('/threads')); navigate(`/threads/${thread.id}`)
    })
  }

  function openThread(thread: Thread) {
    void act(async () => {
      const saved = await request<Thread>(`/threads/${thread.id}`)
      const history = await list<Message>(`/threads/${thread.id}/messages`)
      const activity = await list<ToolRun>(`/threads/${thread.id}/tools`)
      loadedThread.current = saved.id
      setCurrent(saved); setTitle(saved.title); setMessages(history); setDraft(''); navigate(`/threads/${saved.id}`)
      setTools(activity)
    })
  }

  function send() {
    if (!current || !draft.trim() || busy) return
    const content = draft
    const abort = new AbortController(); controller.current = abort; setResponding(true)
    void act(async () => {
      const message = await request<Message>(`/threads/${current.id}/messages`, 'POST', { content }, abort.signal)
      setDraft(''); setMessages(previous => [...previous, message])
      for await (const event of reply(current.id, message.id, abort.signal)) {
        if (event.event === 'delta') setPartial(previous => previous + event.data.text)
        else if (event.event === 'reset') setPartial('')
        else if (event.event === 'done') { setPartial(''); setMessages(previous => [...previous, event.data]) }
        else setTools(previous => previous.some(tool => tool.id === event.data.id) ? previous.map(tool => tool.id === event.data.id ? event.data : tool) : [...previous, event.data])
      }
      const next = await list<Thread>('/threads'); setThreads(next)
      const thread = next.find(thread => thread.id === current.id)
      if (thread) { setCurrent(thread); setTitle(thread.title) }
    }).finally(() => { setResponding(false); if (controller.current === abort) controller.current = null })
  }

  async function stop() {
    if (!current || !responding) return
    try { await request(`/threads/${current.id}/stop`, 'POST') }
    catch (error) { setError(describe(error)) }
    finally { controller.current?.abort(); setPartial(''); setResponding(false) }
  }

  return <div className="app">
    <aside className="sidebar" aria-label="Conversation threads">
      <button className="brand" aria-label="Solmu · Profile" disabled={busy} onClick={() => navigate('/profile')}><Sprout className="brand-mark" strokeWidth={1.5} />solmu</button>
      <Button variant="ghost" className="profile-link" aria-current={isProfile ? 'page' : undefined} disabled={busy} onClick={() => navigate('/profile')}>Profile</Button>
      <div className="sidebar-label">Conversations<Button variant="ghost" size="icon" aria-label="New thread" title="New thread" disabled={busy} onClick={newThread}><Plus size={17} /></Button></div>
      <nav className="threads" aria-label="Thread selector">
        {threads.map(thread => <button key={thread.id} className="thread" aria-current={thread.id === current?.id} disabled={busy} onClick={() => openThread(thread)}><MessageSquare size={14} /><span>{thread.title}</span></button>)}
      </nav>
      <div className="sidebar-footer">YOUR IDEAS, CONNECTED</div>
    </aside>
    <main className="workspace">
      {isProfile ? <ProfilePage revision={profileRevision}/> : <>
      <header className="topbar">
        <Input className="title-input" aria-label="Conversation title" placeholder="Select a conversation" value={title} disabled={busy || !current} onChange={event => setTitle(event.target.value)} />
        <div className="topbar-actions">
          <Button variant="ghost" disabled={!current} onClick={() => { setPluginsOpen(value => !value); setMcpOpen(false); setSkillsOpen(false) }}>Plugins</Button>
          <Button variant="ghost" disabled={!current} onClick={() => { setMcpOpen(value => !value); setSkillsOpen(false); setPluginsOpen(false) }}>MCP</Button>
          <Button variant="ghost" disabled={!current} onClick={() => { setSkillsOpen(value => !value); setMcpOpen(false); setPluginsOpen(false) }}>Skills</Button>
          <Button variant="ghost" aria-label="Select model" disabled={busy || !current} onClick={() => { setCustomModel(current?.model ?? ''); setModelPicker(true) }}>{catalog?.models.find(model => model.id === current?.model)?.name ?? current?.model ?? 'Default model'}</Button>
          <Button size="icon" variant="ghost" aria-label="Rename thread" title="Save title" disabled={busy || !current || !title.trim()} onClick={() => void act(async () => { const thread = await request<Thread>(`/threads/${current!.id}`, 'PATCH', { title }); setCurrent(thread); setTitle(thread.title); setThreads(await list('/threads')) })}><Check size={16} /></Button>
          <Button size="icon" variant="ghost" aria-label="Delete thread" title="Delete thread" disabled={busy || !current} onClick={() => void act(async () => { await request(`/threads/${current!.id}`, 'DELETE'); setCurrent(null); setTitle(''); setMessages([]); setDraft(''); setThreads(await list('/threads')); navigate('/') })}><Trash2 size={15} /></Button>
        </div>
      </header>
      {current?.workspace && <p className="workspace-path" aria-label="Workspace" title={current.workspace}>{current.workspace}</p>}
      {pluginsOpen && current ? <Plugins key={current.id} threadId={current.id} revision={pluginsRevision} onClose={() => setPluginsOpen(false)}/> : mcpOpen && current ? <Mcp key={current.id} threadId={current.id} revision={mcpRevision} onClose={() => setMcpOpen(false)}/> : skillsOpen && current ? <Skills key={current.id} threadId={current.id} revision={skillsRevision} onClose={() => setSkillsOpen(false)}/> : <>
      {modelPicker && <section className="model-picker" role="dialog" aria-label="Select model"><h2>Model for this thread</h2><p>{catalog?.provider ?? 'Configure a provider first'} · Changes apply to the next reply.</p><div className="model-options">{[{ id: '', name: `Default${catalog?.default_model ? ` · ${catalog.default_model}` : ''}` }, ...(catalog?.models ?? [])].map(model => <Button key={model.id} variant={model.id === (current?.model ?? '') ? 'default' : 'outline'} disabled={busy} onClick={() => void act(async () => { const thread = await request<Thread>(`/threads/${current!.id}`, 'PATCH', { model: model.id || null }); setCurrent(thread); setModelPicker(false); await refresh() })}>{model.name}</Button>)}</div><div className="model-custom"><Input aria-label="Custom model ID" placeholder="Custom model ID" value={customModel} disabled={busy} onChange={event => setCustomModel(event.target.value)}/><Button disabled={busy || !customModel.trim()} onClick={() => void act(async () => { const thread = await request<Thread>(`/threads/${current!.id}`, 'PATCH', { model: customModel.trim() }); setCurrent(thread); setModelPicker(false); await refresh() })}>Apply model</Button><Button variant="ghost" onClick={() => setModelPicker(false)}>Cancel</Button></div></section>}
      <section className="history" aria-label="Conversation" aria-busy={busy}>
        {messages.length === 0 && <div className="empty"><span className="eyebrow">Room for possibility</span><h1>A little space for your<br/>next big idea.</h1><p>Ask a question. Untangle a thought. Follow an idea somewhere new — one conversation at a time.</p><div className="suggestions">{['Help me plan a project', 'Explore an idea', 'Explain something new'].map(text => <Button key={text} variant="outline" disabled={!current || busy} onClick={() => setDraft(text)}>{text}</Button>)}</div></div>}
        {messages.map(message => <div key={message.id}><article className="message" data-role={message.role}><div className="message-label">{message.role === 'user' ? 'YOU' : 'SOLMU'}</div><div className="message-content">{message.content}</div></article>{tools.filter(tool => tool.message_id === message.id).map(tool => <details className="tool-activity" key={tool.id}><summary>{tool.name} · {tool.status}</summary><pre>{JSON.stringify(tool.arguments, null, 2)}</pre>{tool.result !== null && <pre>{JSON.stringify(tool.result, null, 2)}</pre>}</details>)}</div>)}
        {partial && <article className="message" data-role="assistant" aria-label="Streaming reply"><div className="message-label">SOLMU<span className="stream-dot"/>STREAMING</div><div className="message-content">{partial}</div></article>}
        <div ref={end}/>
      </section>
      <div className="composer-wrap">
        {error && <p className="error" role="alert">{error}</p>}
        {busy && <p className="working" role="status">Solmu is working…</p>}
        <div className="composer"><Textarea aria-label="Message Solmu" placeholder="Where shall we begin?" value={draft} disabled={busy || !current} onChange={event => setDraft(event.target.value)} onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); send() } }}/>{responding ? <Button className="send" variant="destructive" aria-label="Stop response" onClick={() => void stop()}><Square size={12}/>Stop</Button> : <Button className="send" size="icon" aria-label="Send message" disabled={busy || !current || !draft.trim()} onClick={send}><ArrowUp size={19}/></Button>}</div>
        <div className="composer-note"><span>Enter to send · Shift+Enter for a new line</span>{!connected && <span>Reconnecting…</span>}</div>
      </div>
      </>}
      </>}
    </main>
  </div>
}

function describe(error: unknown) { return error instanceof Error ? error.message : 'Cannot reach Solmu. Check that the backend is running.' }
