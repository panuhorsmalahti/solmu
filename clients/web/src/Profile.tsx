import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { Input } from '@/components/ui/input'
import { request, type Profile } from '@/lib/api'

export default function ProfilePage({ revision }: { revision: number }) {
  const [draft, setDraft] = useState(''), [original, setOriginal] = useState('')
  const [model, setModel] = useState(''), [originalModel, setOriginalModel] = useState('')
  const [backendDefault, setBackendDefault] = useState<string | null>(null)
  const [editedAt, setEditedAt] = useState('')
  const [busy, setBusy] = useState(true), [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const [retry, setRetry] = useState(0)
  const generation = useRef(0)
  const load = useEffectEvent(() => {
    if (draft !== original || model !== originalModel) { setNotice('Profile changed elsewhere. Your draft is unchanged.'); return }
    const requested = ++generation.current
    setBusy(true)
    void request<Profile>('/profile').then(profile => { if (requested !== generation.current) return; setDraft(profile.system_prompt); setOriginal(profile.system_prompt); setModel(profile.model ?? ''); setOriginalModel(profile.model ?? ''); setBackendDefault(profile.backend_default_model); setEditedAt(profile.edited_at); setError('') })
      .catch(error => setError(error instanceof Error ? error.message : 'Cannot load profile'))
      .finally(() => setBusy(false))
  })
  useEffect(() => { const timer = setTimeout(() => load(), 0); return () => clearTimeout(timer) }, [revision, retry])
  useEffect(() => { if (!error || busy || draft !== original || model !== originalModel) return; const timer = setTimeout(() => setRetry(value => value + 1), 2000); return () => clearTimeout(timer) }, [error, busy, draft, original, model, originalModel])
  async function save() {
    setBusy(true); setError(''); setNotice('')
    try {
      const profile = await request<Profile>('/profile', 'PUT', { system_prompt: draft, model: model.trim() || null })
      setOriginal(profile.system_prompt); setDraft(profile.system_prompt); setModel(profile.model ?? ''); setOriginalModel(profile.model ?? ''); setBackendDefault(profile.backend_default_model); setEditedAt(profile.edited_at); setNotice('Profile saved')
    } catch (error) { setError(error instanceof Error ? error.message : 'Cannot save profile') }
    finally { setBusy(false) }
  }
  return <section className="profile-page" aria-label="Profile">
    <span className="eyebrow">MAKE SOLMU YOURS</span><h1>Profile</h1>
    <p>Choose how Solmu approaches your conversations.</p>
    <label htmlFor="system-prompt">System prompt</label>
    <Textarea id="system-prompt" aria-label="System prompt" value={draft} disabled={busy} rows={12} onChange={event => { setDraft(event.target.value); setNotice('') }}/>
    <p className="profile-hint">Shared across all threads and clients. Changes apply to subsequent replies.</p>
    {editedAt && <p className="profile-hint">Edited on <time dateTime={editedAt}>{new Date(editedAt).toLocaleString()}</time></p>}
    <label htmlFor="profile-model">Model (optional)</label><Input id="profile-model" aria-label="Profile model" placeholder={backendDefault ? `${backendDefault} (default)` : 'No model configured'} value={model} disabled={busy} onChange={event => { setModel(event.target.value); setNotice('') }}/><p className="profile-hint">Leave empty to use the backend default. A thread's model selection takes priority.</p>
    {error && <p role="alert" className="error">{error}</p>}
    {notice && <p role="status">{notice}</p>}
    <div className="profile-actions"><Button disabled={busy || !draft.trim()} onClick={() => void save()}>{busy ? 'Loading…' : 'Save profile'}</Button></div>
  </section>
}
