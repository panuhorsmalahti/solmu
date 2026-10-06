import { type FormEvent } from 'react'
import { CirclePlus, LoaderCircle } from 'lucide-react'

type LaunchAgentFormProps = {
  name: string
  creating: boolean
  onNameChange: (name: string) => void
  onSubmit: (event: FormEvent<HTMLFormElement>) => void
}

export function LaunchAgentForm({ name, creating, onNameChange, onSubmit }: LaunchAgentFormProps) {
  return <form className="launch-card" onSubmit={onSubmit}>
    <div className="launch-copy"><div className="launch-icon"><CirclePlus size={19} /></div><div><strong>Launch a Solmu agent</strong><p>A dedicated sandbox will be created in your cluster.</p></div></div>
    <div className="launch-fields"><label><span>AGENT NAME</span><input value={name} onChange={event => onNameChange(event.target.value)} placeholder="e.g. docs-refresh" maxLength={80} required /></label><button className="launch-button" disabled={creating || !name.trim()}>{creating ? <LoaderCircle className="spin" size={16} /> : <CirclePlus size={16} />}{creating ? 'Launching' : 'Launch agent'}</button></div>
  </form>
}
