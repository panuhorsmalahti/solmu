import { useEffect, useRef, useState } from 'react'
import { ArrowUp, MessageSquare, Plus, RefreshCw, Trash2, Check, Sprout } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { list, reply, request, type Thread, type Message } from '@/lib/api'

export default function App() {
  const [threads, setThreads] = useState<Thread[]>([])
  const [current, setCurrent] = useState<Thread | null>(null)
  const [messages, setMessages] = useState<Message[]>([])
  const [draft, setDraft] = useState(''), [title, setTitle] = useState('')
  const [partial, setPartial] = useState(''), [error, setError] = useState('')
  const [busy, setBusy] = useState(true)
  const end = useRef<HTMLDivElement>(null)
  const controller = useRef<AbortController | null>(null)
  const initial = useRef(false)

  useEffect(() => {
    if (initial.current) return
    initial.current = true
    void (async () => {
      try {
        const thread = await request<Thread>('/threads', 'POST', {})
        setCurrent(thread); setTitle(thread.title); setThreads(await list('/threads'))
      } catch (error) { setError(describe(error)) }
      finally { setBusy(false) }
    })()
    return () => { controller.current?.abort() }
  }, [])

  useEffect(() => {
    const timer = window.setInterval(() => {
      if (busy) return
      void list<Thread>('/threads').then(next => {
        setThreads(next)
        const updated = next.find(thread => thread.id === current?.id)
        if (updated) {
          setTitle(value => value === current?.title ? updated.title : value)
          setCurrent(updated)
        }
      }).catch(() => {})
    }, 3000)
    return () => clearInterval(timer)
  }, [busy, current?.id, current?.title])

  useEffect(() => { end.current?.scrollIntoView({ block: 'end' }) }, [messages, partial])

  async function act(operation: () => Promise<void>) {
    if (busy) return
    setBusy(true); setError(''); setPartial('')
    try { await operation() }
    catch (error) { setError(describe(error)); setPartial('') }
    finally { setBusy(false) }
  }

  async function refresh() {
    const next = await list<Thread>('/threads')
    setThreads(next)
    if (current) {
      const thread = await request<Thread>(`/threads/${current.id}`)
      setCurrent(thread); setTitle(thread.title)
      setMessages(await list(`/threads/${current.id}/messages`))
    }
  }

  function newThread() {
    void act(async () => {
      const thread = await request<Thread>('/threads', 'POST', {})
      setCurrent(thread); setTitle(thread.title); setMessages([]); setDraft(''); setThreads(await list('/threads'))
    })
  }

  function openThread(thread: Thread) {
    void act(async () => {
      const saved = await request<Thread>(`/threads/${thread.id}`)
      const history = await list<Message>(`/threads/${thread.id}/messages`)
      setCurrent(saved); setTitle(saved.title); setMessages(history); setDraft('')
    })
  }

  function send() {
    if (!current || !draft.trim() || busy) return
    const content = draft
    void act(async () => {
      const message = await request<Message>(`/threads/${current.id}/messages`, 'POST', { content })
      setDraft(''); setMessages(previous => [...previous, message])
      const abort = new AbortController(); controller.current = abort
      for await (const event of reply(current.id, message.id, abort.signal)) {
        if (event.event === 'delta') setPartial(previous => previous + event.data.text)
        else { setPartial(''); setMessages(previous => [...previous, event.data]) }
      }
      const next = await list<Thread>('/threads'); setThreads(next)
      const thread = next.find(thread => thread.id === current.id)
      if (thread) { setCurrent(thread); setTitle(thread.title) }
    })
  }

  return <div className="app">
    <aside className="sidebar" aria-label="Conversation threads">
      <div className="brand"><Sprout className="brand-mark" strokeWidth={1.5} />solmu</div>
      <div className="sidebar-label">Conversations<Button variant="ghost" size="icon" aria-label="New thread" title="New thread" disabled={busy} onClick={newThread}><Plus size={17} /></Button></div>
      <nav className="threads" aria-label="Thread selector">
        {threads.map(thread => <button key={thread.id} className="thread" aria-current={thread.id === current?.id} disabled={busy} onClick={() => openThread(thread)}><MessageSquare size={14} /><span>{thread.title}</span></button>)}
      </nav>
      <Button variant="ghost" className="justify-start my-3 text-xs" disabled={busy} onClick={() => void act(refresh)}><RefreshCw size={13} />Refresh conversations</Button>
      <div className="sidebar-footer">YOUR IDEAS, CONNECTED</div>
    </aside>
    <main className="workspace">
      <header className="topbar">
        <Input className="title-input" aria-label="Conversation title" placeholder="Select a conversation" value={title} disabled={busy || !current} onChange={event => setTitle(event.target.value)} />
        <div className="topbar-actions">
          <Button size="icon" variant="ghost" aria-label="Rename thread" title="Save title" disabled={busy || !current || !title.trim()} onClick={() => void act(async () => { const thread = await request<Thread>(`/threads/${current!.id}`, 'PATCH', { title }); setCurrent(thread); setTitle(thread.title); setThreads(await list('/threads')) })}><Check size={16} /></Button>
          <Button size="icon" variant="ghost" aria-label="Delete thread" title="Delete thread" disabled={busy || !current} onClick={() => void act(async () => { await request(`/threads/${current!.id}`, 'DELETE'); setCurrent(null); setTitle(''); setMessages([]); setDraft(''); setThreads(await list('/threads')) })}><Trash2 size={15} /></Button>
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
        <div className="composer"><Textarea aria-label="Message Solmu" placeholder="Where shall we begin?" value={draft} disabled={busy || !current} onChange={event => setDraft(event.target.value)} onKeyDown={event => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); send() } }}/><Button className="send" size="icon" aria-label="Send message" disabled={busy || !current || !draft.trim()} onClick={send}><ArrowUp size={19}/></Button></div>
        <div className="composer-note"><span>Enter to send · Shift+Enter for a new line</span><span>Conversations saved locally</span></div>
      </div>
    </main>
  </div>
}

function describe(error: unknown) { return error instanceof Error ? error.message : 'Cannot reach Solmu. Check that the backend is running.' }
