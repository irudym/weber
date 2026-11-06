// Types matching the JSON that weber's Rust handlers send.

export type ProcStatus = {
  name: string
  pid: number | null
  running: boolean
  cpu: number
  mem_bytes: number
  err_count: number
  last_exit: string | null
}

export type LogLine = {
  seq: number
  ts_ms: number
  stream: 'out' | 'err' | 'sys'
  text: string
}

/** fetch() that sends the user to the login page when the session is gone. */
export async function api(url: string, opts?: RequestInit): Promise<Response> {
  const res = await fetch(url, opts)
  if (res.status === 401) {
    location.href = '/login'
    throw new Error('unauthorized')
  }
  return res
}

export function formatBytes(b: number): string {
  if (b >= 1073741824) return (b / 1073741824).toFixed(1) + ' GB'
  if (b >= 1048576) return (b / 1048576).toFixed(1) + ' MB'
  return Math.round(b / 1024) + ' KB'
}

export function formatTime(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour12: false })
}
