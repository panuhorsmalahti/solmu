export interface Thread { id: string; title: string; model: string | null; workspace: string | null }
export interface Profile { system_prompt: string; model: string | null; backend_default_model: string | null; edited_at: string }
export interface ModelCatalog { provider: string | null; default_model: string | null; models: { id: string; name: string }[] }
export interface Message { id: string; role: 'user' | 'assistant'; content: string }
export type ReplyEvent = { event: 'delta'; data: { text: string } } | { event: 'done'; data: Message }

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
        if (event === 'done') { completed = true; yield { event, data: payload } }
      }
      if (done) break
    }
    if (!completed) throw new Error('The reply ended before completion')
  } finally { await reader.cancel().catch(() => {}); reader.releaseLock() }
}
