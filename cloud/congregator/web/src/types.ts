export type Agent = { id: string; name: string; status: string; image: string }
export type Ingress = { name: string; port: number; url: string }
export type AgentAction = 'start' | 'stop' | 'delete'
