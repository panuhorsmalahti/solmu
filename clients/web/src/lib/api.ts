export interface Thread { id: string; title: string; model: string | null; workspace: string | null }
export interface Profile { system_prompt: string; model: string | null; backend_default_model: string | null; edited_at: string }
export interface ModelCatalog { provider: string | null; default_model: string | null; models: { id: string; name: string }[] }
export interface SkillCatalog { directory: string; items: { name: string; description: string; path: string; compatibility: string | null }[]; issues: { path: string; message: string }[] }
export interface McpCatalog { workspace: string; files: string[]; servers: { name: string; source: string; transport: string; status: string; protocol_version: string | null; tools: { name: string; agent_name: string; description: string }[]; error: string | null }[]; issues: { path: string; message: string }[] }
export interface PluginCatalog { directory: string; items: { name: string; version: string | null; description: string | null; path: string; skills: string[]; mcp_servers: string[]; issues: { path: string; message: string }[] }[]; issues: { path: string; message: string }[] }
export interface Message { id: string; role: 'user' | 'assistant'; content: string }
export interface ToolRun { id: string; message_id: string; name: string; arguments: unknown; status: string; result: unknown | null }
export interface AuditRun extends ToolRun { sequence: number; thread_id: string; thread_title: string; call_id: string; created_at: string; started_at: string | null; finished_at: string | null }
export interface CacheSummary { requests: number; input_tokens: number; output_tokens: number; cached_input_tokens: number; cache_creation_input_tokens: number; hit_rate_percent: number | null }
export interface AuditPage { items: AuditRun[]; next_cursor: number | null; cache_24h: CacheSummary }
export interface ScheduledTask { id: string; name: string; prompt: string; schedule_kind: 'once' | 'cron'; schedule: string; thread_id: string; enabled: boolean; running: boolean; next_run_at: string | null; last_run_at: string | null; last_status: string | null }
export interface TaskRun { id: string; scheduled_for: string; started_at: string; finished_at: string | null; status: string; error: string | null }
export type ReplyEvent = { event: 'delta'; data: { text: string } } | { event: 'done'; data: Message } | { event: 'reset'; data: object } | { event: 'tool_start' | 'tool_result'; data: ToolRun }

export async function request<T>(path: string, method = 'GET', body?: unknown, signal?: AbortSignal): Promise<T> {
  const response = await fetch(`/api/v1${path}`, {
    method, headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body), signal: signal ?? AbortSignal.timeout(30_000),
  })
  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(error?.error?.message ?? `Request failed (${response.status})`)
  }
  return response.status === 204 ? undefined as T : response.json()
}

export async function list<T>(path: string): Promise<T[]> {
  const items: T[] = []
  for (;;) {
    const page = await request<{ items: T[] }>(`${path}?limit=100&offset=${items.length}`)
    items.push(...page.items)
    if (page.items.length < 100) return items
  }
}

export async function* reply(thread: string, message: string, signal: AbortSignal): AsyncGenerator<ReplyEvent> {
  const response = await fetch(`/api/v1/threads/${thread}/responses`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ message_id: message }), signal,
  })
  if (!response.ok) {
    const error = await response.json().catch(() => ({}))
    throw new Error(error?.error?.message ?? `Reply failed (${response.status})`)
  }
  if (!response.body) throw new Error('Streaming is unavailable in this browser')
  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = '', completed = false
  try {
    for (;;) {
      const { value, done } = await reader.read()
      buffer += decoder.decode(value, { stream: !done })
      let boundary: RegExpExecArray | null
      while ((boundary = /\r?\n\r?\n/.exec(buffer))) {
        const frame = buffer.slice(0, boundary.index)
        buffer = buffer.slice(boundary.index + boundary[0].length)
        let event = '', data = ''
        for (const line of frame.split(/\r?\n/)) {
          if (line.startsWith('event:')) event = line.slice(6).trim()
          if (line.startsWith('data:')) data += `${line.slice(5).replace(/^ /, '')}\n`
        }
        if (!data) continue
        const payload = JSON.parse(data)
        if (event === 'error') throw new Error(payload.error?.message ?? 'The reply failed')
        if (event === 'stopped') return
        if (event === 'delta') yield { event, data: payload }
        if (event === 'reset' || event === 'tool_start' || event === 'tool_result') yield { event, data: payload }
        if (event === 'done') { completed = true; yield { event, data: payload } }
      }
      if (done) break
    }
    if (!completed) throw new Error('The reply ended before completion')
  } finally { await reader.cancel().catch(() => {}); reader.releaseLock() }
}
