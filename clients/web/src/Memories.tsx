import { useCallback, useEffect, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { request, type Memory, type MemoryPage } from '@/lib/api'

const PAGE_SIZE = 30

export default function Memories({ revision }: { revision: number }) {
  const [items, setItems] = useState<Memory[]>([])
  const [hasMore, setHasMore] = useState(true), [loading, setLoading] = useState(false)
  const [draft, setDraft] = useState(''), [editing, setEditing] = useState<string | null>(null)
  const [error, setError] = useState(''), [notice, setNotice] = useState('')
  const sentinel = useRef<HTMLDivElement>(null)
  const loadingRef = useRef(false), offsetRef = useRef(0), hasMoreRef = useRef(true)
  const loadPage = useCallback(async (reset = false) => {
    if (loadingRef.current || (!reset && !hasMoreRef.current)) return
    loadingRef.current = true
    setLoading(true); setError('')
    const nextOffset = reset ? 0 : offsetRef.current
    try {
      const page = await request<MemoryPage>(`/memories?limit=${PAGE_SIZE}&offset=${nextOffset}`)
      setItems(current => reset ? page.items : [...current, ...page.items.filter(memory => !current.some(item => item.id === memory.id))])
      offsetRef.current = nextOffset + page.items.length; hasMoreRef.current = page.has_more
      setHasMore(page.has_more)
    } catch (err) { setError(err instanceof Error ? err.message : 'Cannot load memories') }
    finally { loadingRef.current = false; setLoading(false) }
  }, [])
  useEffect(() => { void loadPage(true) }, [revision, loadPage])
  useEffect(() => {
    const node = sentinel.current
    if (!node || !hasMore || loading) return
    const observer = new IntersectionObserver(entries => { if (entries.some(entry => entry.isIntersecting)) void loadPage() }, { rootMargin: '240px' })
    observer.observe(node); return () => observer.disconnect()
  }, [hasMore, loading, loadPage])
  async function save() {
    if (!draft.trim()) return
    try {
      const saved = await request<Memory>(editing ? `/memories/${editing}` : '/memories', editing ? 'PUT' : 'POST', { content: draft.trim() })
      setItems(current => editing ? current.map(item => item.id === editing ? saved : item) : [saved, ...current])
      setDraft(''); setEditing(null); setNotice(editing ? 'Memory updated' : 'Memory saved'); setError('')
    } catch (err) { setError(err instanceof Error ? err.message : 'Cannot save memory') }
  }
  async function remove(id: string) {
    try { await request(`/memories/${id}`, 'DELETE'); setItems(current => current.filter(item => item.id !== id)); setNotice('Memory deleted') }
    catch (err) { setError(err instanceof Error ? err.message : 'Cannot delete memory') }
  }
  return <section className="memories-page" aria-label="Memories">
    <span className="eyebrow">WHAT SOLMU REMEMBERS</span><h1>Memories</h1>
    <p>Saved facts can be recalled in any conversation when they match what you ask.</p>
    <div className="memory-editor"><Textarea aria-label="Memory content" placeholder="A fact Solmu should remember" value={draft} rows={3} maxLength={20000} onChange={event => setDraft(event.target.value)}/><div className="memory-actions"><Button disabled={!draft.trim()} onClick={() => void save()}>{editing ? 'Save changes' : 'Add memory'}</Button>{editing && <Button variant="ghost" onClick={() => { setEditing(null); setDraft('') }}>Cancel</Button>}</div></div>
    {error && <p role="alert" className="error">{error}</p>}{notice && <p role="status">{notice}</p>}
    {items.length === 0 && !loading && <p className="memory-empty">No saved memories yet.</p>}
    <div className="memory-list">{items.map(memory => <article className="memory-card" key={memory.id}><p>{memory.content}</p><div className="memory-meta"><time dateTime={memory.created_at}>Saved {new Date(memory.created_at).toLocaleString()}</time><div><Button size="sm" variant="ghost" onClick={() => { setEditing(memory.id); setDraft(memory.content); setNotice('') }}>Edit</Button><Button size="sm" variant="ghost" onClick={() => void remove(memory.id)}>Delete</Button></div></div></article>)}</div>
    <div ref={sentinel} aria-hidden="true"/>{loading && <p role="status" className="memory-loading">Loading memories…</p>}{!loading && hasMore && items.length > 0 && <Button variant="outline" onClick={() => void loadPage()}>Load more</Button>}
  </section>
}
