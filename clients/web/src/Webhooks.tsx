import { useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { request, type Webhook } from '@/lib/api'

type Draft = Pick<Webhook, 'name' | 'auth_type' | 'instructions'> & { secret: string }
const blank: Draft = { name: '', auth_type: 'github-hmac-sha256', instructions: '', secret: '' }

function HookCard({ webhook, onChange }: { webhook: Webhook; onChange: (hook: Webhook) => void }) {
  const [draft, setDraft] = useState<Partial<Draft>>({})
  const [enabledDraft, setEnabledDraft] = useState<boolean | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const value: Draft = { name: draft.name ?? webhook.name, auth_type: draft.auth_type ?? webhook.auth_type, instructions: draft.instructions ?? webhook.instructions, secret: draft.secret ?? '' }
  const enabled = enabledDraft ?? webhook.enabled
  const dirty = Object.keys(draft).length > 0
  const edit = (key: keyof Draft, next: string) => { setDraft(current => ({ ...current, [key]: next })); setError('') }
  const save = async (extra: { enabled?: boolean } = {}) => {
    setBusy(true); setError('')
    try {
      const body = { name: value.name, auth_type: value.auth_type, instructions: value.instructions, ...(value.secret ? { secret: value.secret } : {}), ...extra }
      const saved = await request<Webhook>(`/webhooks/${webhook.id}`, 'PATCH', body)
      setDraft({}); setEnabledDraft(null); onChange(saved)
      return true
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Cannot save webhook') }
    finally { setBusy(false) }
    return false
  }
  const endpoint = `${window.location.protocol}//${window.location.hostname}:${webhook.port}/hooks/${webhook.id}`

  return <article className="webhook-card">
    <div className="webhook-top"><div><span className={`webhook-state${webhook.enabled ? ' is-on' : ''}`}>{webhook.enabled ? 'Enabled' : 'Disabled'}</span><h2>{webhook.name}</h2></div>
      <label className="webhook-toggle"><input aria-label={`Enable ${webhook.name}`} type="checkbox" checked={enabled} disabled={busy} onChange={event => { const next = event.target.checked; setEnabledDraft(next); void save({ enabled: next }).then(saved => { if (!saved) setEnabledDraft(null) }) }}/><span>Enabled</span></label>
    </div>
    <label htmlFor={`hook-endpoint-${webhook.id}`}>Webhook URL</label><code id={`hook-endpoint-${webhook.id}`} className="webhook-endpoint">{endpoint}</code>
    <label htmlFor={`hook-name-${webhook.id}`}>Name</label><Input id={`hook-name-${webhook.id}`} value={value.name} disabled={busy} onChange={event => edit('name', event.target.value)}/>
    <label htmlFor={`hook-auth-${webhook.id}`}>Authentication</label><select id={`hook-auth-${webhook.id}`} value={value.auth_type} disabled={busy} onChange={event => edit('auth_type', event.target.value as Draft['auth_type'])}><option value="github-hmac-sha256">GitHub signature (HMAC-SHA256)</option><option value="bearer">Bearer token</option></select>
    <label htmlFor={`hook-secret-${webhook.id}`}>Replace secret {webhook.secret_configured && <span className="webhook-muted">(configured; leave blank to keep it)</span>}</label><Input id={`hook-secret-${webhook.id}`} type="password" autoComplete="new-password" minLength={16} value={value.secret} disabled={busy} onChange={event => edit('secret', event.target.value)} placeholder="At least 16 characters"/>
    <label htmlFor={`hook-instructions-${webhook.id}`}>Instructions for Solmu</label><Textarea id={`hook-instructions-${webhook.id}`} value={value.instructions} disabled={busy} onChange={event => edit('instructions', event.target.value)} placeholder="What should Solmu do with this event?"/>
    {error && <p className="error" role="alert">{error}</p>}
    <div className="webhook-actions"><Button disabled={busy || !value.name.trim()} onClick={() => void save()}>{busy ? 'Saving…' : 'Save changes'}</Button><Button variant="outline" disabled={busy || !dirty} onClick={() => { setDraft({}); setEnabledDraft(null); setError('') }}>Discard edits</Button></div>
  </article>
}

export default function Webhooks({ revision }: { revision: number }) {
  const [items, setItems] = useState<Webhook[]>([])
  const [draft, setDraft] = useState<Draft>(blank)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  useEffect(() => {
    let disposed = false
    void request<Webhook[]>('/webhooks').then(value => { if (!disposed) setItems(value) }).catch(cause => { if (!disposed) setError(cause instanceof Error ? cause.message : 'Cannot load webhooks') })
    return () => { disposed = true }
  }, [revision])

  async function create() {
    setBusy(true); setError('')
    try {
      const item = await request<Webhook>('/webhooks', 'POST', draft)
      setItems(values => [item, ...values]); setDraft(blank)
    } catch (cause) { setError(cause instanceof Error ? cause.message : 'Cannot create webhook') }
    finally { setBusy(false) }
  }
  async function remove(webhook: Webhook) {
    setError('')
    try { await request(`/webhooks/${webhook.id}`, 'DELETE'); setItems(values => values.filter(value => value.id !== webhook.id)) }
    catch (cause) { setError(cause instanceof Error ? cause.message : 'Cannot delete webhook') }
  }

  return <section className="webhooks-page" aria-label="Webhooks">
    <div className="webhooks-inner"><span className="eyebrow">LET EVENTS START A THREAD</span><h1>Webhooks</h1>
      <p>Connect GitHub or another service. An authenticated event starts a new conversation, and Solmu replies in the background.</p>
      {error && <p className="error" role="alert">{error}</p>}
      <section className="webhook-create" aria-label="Add a webhook"><h2>Add a webhook</h2>
        <p className="webhook-muted">The secret is saved by Solmu and won’t be shown again. Enter the same secret in your service’s webhook settings.</p>
        <label htmlFor="new-hook-name">Name</label><Input id="new-hook-name" value={draft.name} disabled={busy} onChange={event => setDraft(value => ({ ...value, name: event.target.value }))} placeholder="GitHub repository"/>
        <label htmlFor="new-hook-auth">Authentication</label><select id="new-hook-auth" value={draft.auth_type} disabled={busy} onChange={event => setDraft(value => ({ ...value, auth_type: event.target.value as Draft['auth_type'] }))}><option value="github-hmac-sha256">GitHub signature (HMAC-SHA256)</option><option value="bearer">Bearer token</option></select>
        <label htmlFor="new-hook-secret">Secret</label><Input id="new-hook-secret" type="password" autoComplete="new-password" minLength={16} value={draft.secret} disabled={busy} onChange={event => setDraft(value => ({ ...value, secret: event.target.value }))} placeholder="At least 16 characters"/>
        <label htmlFor="new-hook-instructions">Instructions for Solmu</label><Textarea id="new-hook-instructions" value={draft.instructions} disabled={busy} onChange={event => setDraft(value => ({ ...value, instructions: event.target.value }))} placeholder="Summarize issues and suggest next steps"/>
        <div className="webhook-actions"><Button disabled={busy || !draft.name.trim() || draft.secret.length < 16} onClick={() => void create()}>{busy ? 'Creating…' : 'Create webhook'}</Button></div>
      </section>
      <h2 className="webhook-list-title">Your webhooks</h2>
      {items.length === 0 ? <p className="webhook-muted">No webhooks yet. New webhooks start disabled.</p> : items.map(webhook => <div key={webhook.id}><HookCard webhook={webhook} onChange={changed => setItems(values => values.map(value => value.id === changed.id ? changed : value))}/><div className="webhook-delete"><Button variant="ghost" onClick={() => void remove(webhook)}>Delete {webhook.name}</Button></div></div>)}
    </div>
  </section>
}
