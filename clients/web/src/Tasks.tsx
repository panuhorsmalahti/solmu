import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { request, type ScheduledTask, type TaskRun } from '@/lib/api'

function localTime(date: Date) {
  return new Date(date.getTime() - date.getTimezoneOffset() * 60_000).toISOString().slice(0, 16)
}
function formatTime(value: string | null) { return value ? new Date(value).toLocaleString() : 'Never' }

export default function Tasks({ revision }: { revision: number }) {
  const [items, setItems] = useState<ScheduledTask[]>([])
  const [runs, setRuns] = useState<Record<string, TaskRun[]>>({})
  const [selected, setSelected] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const [name, setName] = useState(''), [prompt, setPrompt] = useState('')
  const [kind, setKind] = useState<'once' | 'cron'>('once')
  const [at, setAt] = useState(() => localTime(new Date(Date.now() + 3_600_000)))
  const [cron, setCron] = useState('0 9 * * *')
  const [busy, setBusy] = useState(false), [error, setError] = useState('')
  const [refresh, setRefresh] = useState(0)

  useEffect(() => {
    let active = true
    void request<{ items: ScheduledTask[] }>('/tasks').then(page => { if (active) { setItems(page.items); setError('') } })
      .catch(cause => { if (active) setError(cause instanceof Error ? cause.message : 'Cannot load tasks') })
    return () => { active = false }
  }, [revision, refresh])
  useEffect(() => {
    if (!selected) return
    let active = true
    void request<{ items: TaskRun[] }>(`/tasks/${selected}/runs`).then(page => { if (active) setRuns(old => ({ ...old, [selected]: page.items })) })
      .catch(cause => { if (active) setError(cause instanceof Error ? cause.message : 'Cannot load task runs') })
    return () => { active = false }
  }, [selected, revision, refresh])

  function reset() {
    setEditing(null); setName(''); setPrompt(''); setKind('once')
    setAt(localTime(new Date(Date.now() + 3_600_000))); setCron('0 9 * * *')
  }
  function edit(task: ScheduledTask) {
    setEditing(task.id); setName(task.name); setPrompt(task.prompt); setKind(task.schedule_kind)
    if (task.schedule_kind === 'once') setAt(localTime(new Date(task.schedule)))
    else setCron(task.schedule)
    document.querySelector('.tasks-page')?.scrollTo({ top: 0, behavior: 'smooth' })
  }
  async function act(action: () => Promise<unknown>) {
    if (busy) return
    setBusy(true); setError('')
    try { await action(); setRefresh(value => value + 1) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'Task request failed') }
    finally { setBusy(false) }
  }
  function save() {
    const date = new Date(at)
    if (!name.trim() || !prompt.trim() || (kind === 'once' && Number.isNaN(new Date(at).getTime()))) {
      setError('Enter a name, prompt, and valid schedule'); return
    }
    const schedule = kind === 'cron' ? cron.trim() : date.toISOString()
    void act(async () => {
      const body = { name: name.trim(), prompt: prompt.trim(), schedule_kind: kind, schedule }
      if (editing) await request(`/tasks/${editing}`, 'PATCH', body)
      else await request('/tasks', 'POST', body)
      reset()
    })
  }

  return <section className="tasks-page" aria-label="Scheduled tasks">
    <div className="tasks-inner"><span className="eyebrow">WORK THAT KEEPS GOING</span><h1>Tasks</h1>
      <p>Schedule Solmu to work later. Each task has its own saved conversation and run history.</p>
      <div className="task-form"><h2>{editing ? 'Edit task' : 'New task'}</h2>
        <label htmlFor="task-name">Name</label><Input id="task-name" value={name} onChange={event => setName(event.target.value)} placeholder="Morning review"/>
        <label htmlFor="task-prompt">What should Solmu do?</label><Textarea id="task-prompt" value={prompt} onChange={event => setPrompt(event.target.value)} rows={3} placeholder="Review the workspace and summarize changes"/>
        <div className="task-schedule"><label htmlFor="task-kind">Schedule</label><select id="task-kind" value={kind} onChange={event => setKind(event.target.value as 'once' | 'cron')}><option value="once">One time</option><option value="cron">Recurring cron</option></select>
          {kind === 'once' ? <><label htmlFor="task-at">Run at</label><Input id="task-at" type="datetime-local" value={at} onChange={event => setAt(event.target.value)}/></> : <><label htmlFor="task-cron">Cron expression (UTC)</label><Input id="task-cron" value={cron} onChange={event => setCron(event.target.value)} placeholder="0 9 * * *"/><small>Minute hour day month weekday. Times use UTC.</small></>}
        </div>
        <div className="task-actions"><Button disabled={busy || !name.trim() || !prompt.trim()} onClick={save}>{editing ? 'Save task' : 'Create task'}</Button>{editing && <Button variant="ghost" onClick={reset}>Cancel</Button>}</div>
      </div>
      {error && <p role="alert" className="error">{error}</p>}
      <h2 className="tasks-list-title">Scheduled tasks</h2>
      {items.length === 0 && <p>No tasks yet.</p>}
      {items.map(task => <article className="task-card" key={task.id}>
        <div className="task-card-top"><div><h3>{task.name}</h3><span className="task-badge">{task.running ? 'Running' : task.enabled ? 'Scheduled' : task.schedule_kind === 'once' && task.last_status === 'completed' ? 'Completed' : 'Paused'}</span></div><Link to={`/threads/${task.thread_id}`}>Open conversation</Link></div>
        <p>{task.prompt}</p><dl><div><dt>Schedule</dt><dd>{task.schedule_kind === 'cron' ? `${task.schedule} · UTC` : formatTime(task.schedule)}</dd></div><div><dt>Next run</dt><dd>{task.enabled ? formatTime(task.next_run_at) : '—'}</dd></div><div><dt>Last run</dt><dd>{formatTime(task.last_run_at)}{task.last_status ? ` · ${task.last_status}` : ''}</dd></div></dl>
        <div className="task-actions"><Button variant="outline" disabled={busy || task.running} onClick={() => void act(() => request(`/tasks/${task.id}/run`, 'POST'))}>Run now</Button>
          {task.enabled ? <Button variant="ghost" disabled={busy || task.running} onClick={() => void act(() => request(`/tasks/${task.id}`, 'PATCH', { enabled: false }))}>Pause</Button> : task.schedule_kind === 'cron' || task.last_status !== 'completed' ? <Button variant="ghost" disabled={busy || task.running} onClick={() => void act(() => request(`/tasks/${task.id}`, 'PATCH', { enabled: true }))}>Resume</Button> : null}
          <Button variant="ghost" disabled={busy || task.running} onClick={() => edit(task)}>Edit</Button><Button variant="ghost" disabled={busy || task.running} onClick={() => void act(() => request(`/tasks/${task.id}`, 'DELETE'))}>Delete</Button>
          <Button variant="ghost" onClick={() => setSelected(value => value === task.id ? null : task.id)}>{selected === task.id ? 'Hide runs' : 'Run history'}</Button></div>
        {selected === task.id && <div className="task-runs">{(runs[task.id] ?? []).length === 0 ? <p>No runs yet.</p> : runs[task.id].map(run => <div key={run.id}><strong>{run.status}</strong> · {formatTime(run.started_at)}{run.error && <p role="alert">{run.error}</p>}</div>)}</div>}
      </article>)}
    </div>
  </section>
}
