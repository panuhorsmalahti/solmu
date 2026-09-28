import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { request, type AuditPage, type AuditRun } from '@/lib/api'

const PAGE_SIZE = 25

export default function Audit({ revision }: { revision: number }) {
  const [items, setItems] = useState<AuditRun[]>([])
  const [cursor, setCursor] = useState<number | null>(null)
  const [busy, setBusy] = useState(true)
  const [error, setError] = useState('')
  const itemsRef = useRef<AuditRun[]>([])
  const cursorRef = useRef<number | null>(null)
  const generation = useRef(0)
  const loading = useRef(false)
  const scroll = useRef<HTMLElement>(null)
  const sentinel = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const current = ++generation.current
    let disposed = false
    void (async () => {
      setBusy(true)
      try {
        const count = Math.max(PAGE_SIZE, itemsRef.current.length)
        const fresh: AuditRun[] = []
        let next: number | null = null
        do {
          const page: AuditPage = await request(`/audit?limit=${PAGE_SIZE}${next === null ? '' : `&before=${next}`}`)
          fresh.push(...page.items)
          next = page.next_cursor
        } while (next !== null && fresh.length < count)
        if (!disposed && current === generation.current) {
          itemsRef.current = fresh; cursorRef.current = next
          setItems(fresh); setCursor(next); setError('')
        }
      } catch (cause) {
        if (!disposed && current === generation.current) setError(cause instanceof Error ? cause.message : 'Cannot load audit')
      } finally { if (!disposed && current === generation.current) setBusy(false) }
    })()
    return () => { disposed = true }
  }, [revision])

  useEffect(() => {
    if (busy || !cursor || !sentinel.current || !scroll.current) return
    const observer = new IntersectionObserver(entries => {
      if (!entries[0].isIntersecting || loading.current || cursorRef.current === null) return
      loading.current = true
      const current = generation.current
      const before = cursorRef.current
      setBusy(true)
      void request<AuditPage>(`/audit?limit=${PAGE_SIZE}&before=${before}`).then(page => {
        if (current !== generation.current) return
        const seen = new Set(itemsRef.current.map(item => item.id))
        const merged = [...itemsRef.current, ...page.items.filter(item => !seen.has(item.id))]
        itemsRef.current = merged; cursorRef.current = page.next_cursor
        setItems(merged); setCursor(page.next_cursor); setError('')
      }).catch(cause => { if (current === generation.current) setError(cause instanceof Error ? cause.message : 'Cannot load older calls') })
        .finally(() => { loading.current = false; if (current === generation.current) setBusy(false) })
    }, { root: scroll.current, rootMargin: '250px' })
    observer.observe(sentinel.current)
    return () => observer.disconnect()
  }, [cursor, items.length, busy])

  return <section className="audit-page" aria-label="Tool call audit" ref={scroll}>
    <div className="audit-inner"><span className="eyebrow">THE WORK BEHIND THE WORK</span><h1>Audit</h1>
      <p className="audit-intro">Tool calls from every conversation, newest first. Open a call to inspect its arguments and result.</p>
      {items.length === 0 && !busy && !error && <p>No tool calls yet.</p>}
      {items.map(item => <article className="audit-card" key={item.id} data-audit-id={item.id}>
        <details><summary><span className="audit-name">{item.name}</span><span className={`audit-status audit-${item.status}`}>{item.status}</span><span className="audit-time">{new Date(item.created_at).toLocaleString()}</span></summary>
          <div className="audit-detail"><div><strong>Arguments</strong><pre>{JSON.stringify(item.arguments, null, 2)}</pre></div><div><strong>Result</strong><pre>{item.result === null ? 'Pending' : JSON.stringify(item.result, null, 2)}</pre></div>
            <p>Call ID: {item.call_id}<br/>Started: {item.started_at ?? 'Pending'}<br/>Finished: {item.finished_at ?? 'Pending'}</p></div>
        </details><Link to={`/threads/${item.thread_id}`} className="audit-thread">{item.thread_title}</Link>
      </article>)}
      {error && <p role="alert" className="error">{error}</p>}
      {busy && <p role="status" className="audit-loading">Loading tool calls…</p>}
      {!busy && cursor === null && items.length > 0 && <p className="audit-end">All tool calls shown.</p>}
      <div ref={sentinel} className="audit-sentinel" aria-hidden="true" />
    </div>
  </section>
}
