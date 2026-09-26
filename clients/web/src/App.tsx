import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { ArrowUp, MessageSquare, Plus, Square, Trash2, Check, Sprout } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { list, reply, request, type Thread, type Message } from '@/lib/api'

export default function App() {
  const { threadId } = useParams()
  const navigate = useNavigate()
  const [threads, setThreads] = useState<Thread[]>([])
  const [current, setCurrent] = useState<Thread | null>(null)
  const [messages, setMessages] = useState<Message[]>([])
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
        if (requestedRevision !== revision.current) { dirty.current = true; return }
        setTitle(value => value === current.title ? thread.title : value); setCurrent(thread)
        setMessages(history)
      } else { loadedThread.current = null; setCurrent(null); setTitle(''); setMessages([]); navigate('/', { replace: true }) }
    }
    setThreads(next)
  }

  useEffect(() => {
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
        const next: Thread[] = await list('/threads')
        if (disposed) return
        initial.current = true
        loadedThread.current = thread?.id ?? null
        setCurrent(thread); setTitle(thread?.title ?? ''); setMessages(history); setThreads(next); setDraft('')
        if (thread && !threadId) navigate(`/threads/${thread.id}`, { replace: true })
      } catch (error) { setError(describe(error)) }
      finally { if (!disposed) setBusy(false) }
    })()
    return () => {
      disposed = true
      if (controller.current) { controller.current.abort(); if (threadId) void request(`/threads/${threadId}/stop`, 'POST').catch(() => {}) }
    }
  }, [threadId, navigate])

  const liveChange = useEffectEvent(() => {
    if (busy) { dirty.current = true; return }
    void refresh().catch(error => setError(describe(error)))
  })
  useEffect(() => {
    let socket: WebSocket | undefined, timer: ReturnType<typeof setTimeout> | undefined, disposed = false
    function connect() {
      socket = new WebSocket(`${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/api/v1/events`)
      socket.onopen = () => { setConnected(true); liveChange() }
      socket.onmessage = event => { if (JSON.parse(event.data).type === 'conversation_changed') liveChange() }
      socket.onclose = () => { if (!disposed) { setConnected(false); timer = setTimeout(connect, 2000) } }
      socket.onerror = () => socket?.close()
    }
    connect()
    return () => { disposed = true; clearTimeout(timer); socket?.close(); controller.current?.abort() }
  }, [])
  useEffect(() => { if (!busy && dirty.current) { dirty.current = false; liveChange() } }, [busy])

  useEffect(() => { end.current?.scrollIntoView({ block: 'end' }) }, [messages, partial])

  async function act(operation: () => Promise<void>) {
    if (busy) return
    ++revision.current
    setBusy(true); setError(''); setPartial('')
    try { await operation() }
    catch (error) { if (!(error instanceof Error && (error.name === 'AbortError' || error.message === 'Response stopped'))) setError(describe(error)); setPartial('') }
    finally { setBusy(false) }
  }

  function newThread() {
    void act(async () => {
      const thread = await request<Thread>('/threads', 'POST', {})
      loadedThread.current = thread.id
      setCurrent(thread); setTitle(thread.title); setMessages([]); setDraft(''); setThreads(await list('/threads')); navigate(`/threads/${thread.id}`)
    })
  }

  function openThread(thread: Thread) {
    void act(async () => {
      const saved = await request<Thread>(`/threads/${thread.id}`)
      const history = await list<Message>(`/threads/${thread.id}/messages`)
      loadedThread.current = saved.id
      setCurrent(saved); setTitle(saved.title); setMessages(history); setDraft(''); navigate(`/threads/${saved.id}`)
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
        else { setPartial(''); setMessages(previous => [...previous, event.data]) }
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
      <div className="brand"><Sprout className="brand-mark" strokeWidth={1.5} />solmu</div>
      <div className="sidebar-label">Conversations<Button variant="ghost" size="icon" aria-label="New thread" title="New thread" disabled={busy} onClick={newThread}><Plus size={17} /></Button></div>
      <nav className="threads" aria-label="Thread selector">
        {threads.map(thread => <button key={thread.id} className="thread" aria-current={thread.id === current?.id} disabled={busy} onClick={() => openThread(thread)}><MessageSquare size={14} /><span>{thread.title}</span></button>)}
      </nav>
      <div className="sidebar-footer">YOUR IDEAS, CONNECTED</div>
    </aside>
    <main className="workspace">
      <header className="topbar">
        <Input className="title-input" aria-label="Conversation title" placeholder="Select a conversation" value={title} disabled={busy || !current} onChange={event => setTitle(event.target.value)} />
        <div className="topbar-actions">
          <Button size="icon" variant="ghost" aria-label="Rename thread" title="Save title" disabled={busy || !current || !title.trim()} onClick={() => void act(async () => { const thread = await request<Thread>(`/threads/${current!.id}`, 'PATCH', { title }); setCurrent(thread); setTitle(thread.title); setThreads(await list('/threads')) })}><Check size={16} /></Button>
          <Button size="icon" variant="ghost" aria-label="Delete thread" title="Delete thread" disabled={busy || !current} onClick={() => void act(async () => { await request(`/threads/${current!.id}`, 'DELETE'); setCurrent(null); setTitle(''); setMessages([]); setDraft(''); setThreads(await list('/threads')); navigate('/') })}><Trash2 size={15} /></Button>
        </div>
      </header>
      <section className="history" aria-label="Conversation" aria-busy={busy}>
        {messages.length === 0 && <div className="empty"><span className="eyebrow">Room for possibility</span><h1>A little space for your<br/>next big idea.</h1><p>Ask a question. Untangle a thought. Follow an idea somewhere new — one conversation at a time.</p><div className="suggestions">{['Help me plan a project', 'Explore an idea', 'Explain something new'].map(text => <Button key={text} variant="outline" disabled={!current || busy} onClick={() => setDraft(text)}>{text}</Button>)}</div></div>}
        {messages.map(message => <article className="message" key={message.id} data-role={message.role}><div className="message-label">{message.role === 'user' ? 'YOU' : 'SOLMU'}</div><div className="message-content">{message.content}</div></article>)}
        {partial && <article className="message" data-role="assistant" aria-label="Streaming reply"><div className="message-label">SOLMU<span className="stream-dot"/>STREAMING</div><div className="message-content">{partial}</div></article>}
        <div ref={end}/>
      </section>
      <div className="composer-wrap">
        {error && <p className="error" role="alert">{error}</p>}
        {busy && <p className="working" role="status">Solmu is working…</p>}
        <div className="composer"><Textarea aria-label="Message Solmu" placeholder="Where shall we begin?" value={draft} disabled={busy || !current} onChange={event => setDraft(event.target.value)} onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); send() } }}/>{responding ? <Button className="send" variant="destructive" aria-label="Stop response" onClick={() => void stop()}><Square size={12}/>Stop</Button> : <Button className="send" size="icon" aria-label="Send message" disabled={busy || !current || !draft.trim()} onClick={send}><ArrowUp size={19}/></Button>}</div>
        <div className="composer-note"><span>Enter to send · Shift+Enter for a new line</span><span>{connected ? 'Conversations saved locally' : 'Reconnecting…'}</span></div>
      </div>
    </main>
  </div>
}

function describe(error: unknown) { return error instanceof Error ? error.message : 'Cannot reach Solmu. Check that the backend is running.' }
