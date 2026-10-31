import { useEffect, useState } from 'react'

import { wsUrl } from '@/lib/api'
import type { Overview, TickPoint, TickSeries, WsFrame } from '@/lib/types'

export type WsStatus = 'connecting' | 'open' | 'closed' | 'error'

export interface MetricsWs {
  status: WsStatus
  overview: Overview | null
  ticks: TickSeries | null
  /** The last change-feed frame: a topic changed, so a page refetches. */
  live: LiveMark | null
  /** Wall-clock ms of the last frame applied, `null` until the first one lands. */
  lastFrameAtMs: number | null
  frames: number
  dropped: number
  reconnects: number
}

/** One change on one topic; the running sequence is what a page watches. */
export interface LiveMark {
  topic: string
  seq: number
}

const MIN_BACKOFF_MS = 500
const MAX_BACKOFF_MS = 30_000
// An `open` socket quieter than this is half-open and gets replaced.
const WATCHDOG_MS = 5_000
// `engine_dashboard::WS_PROTOCOL`.
const SUPPORTED_PROTOCOL = 1
// `engine_dashboard::HISTORY_LEN`.
const MAX_POINTS = 300

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

// A frame is either something this hook can apply, a protocol it has never
// seen, or noise. Only the first is applied; the second stops the retry loop.
type Parsed = { kind: 'frame'; frame: WsFrame } | { kind: 'noise' } | { kind: 'protocol' }

function parseFrame(raw: string): Parsed {
  if (raw === '') return { kind: 'noise' }
  let value: unknown
  try {
    value = JSON.parse(raw)
  } catch {
    return { kind: 'noise' }
  }
  if (!isRecord(value)) return { kind: 'noise' }
  const type = value['type']
  if (type === 'goodbye') return { kind: 'frame', frame: value as unknown as WsFrame }
  // A change notification: it carries no telemetry, but it is a frame the page
  // turns into a cache invalidation.
  if (type === 'live') return { kind: 'frame', frame: value as unknown as WsFrame }
  if (type !== 'hello' && type !== 'snapshot') return { kind: 'noise' }
  if (!isRecord(value['overview']) || !isRecord(value['ticks'])) return { kind: 'noise' }
  if (type === 'hello' && value['protocol'] !== SUPPORTED_PROTOCOL) {
    return { kind: 'protocol' }
  }
  return { kind: 'frame', frame: value as unknown as WsFrame }
}

function pointKey(p: TickPoint): string {
  return `${p.seq}:${p.atMs}`
}

// A snapshot carries the plane's whole ring, so incoming points win where they
// overlap; appending is only for the run this page saw that the ring evicted.
function mergePoints(prev: TickPoint[] | undefined, incoming: TickPoint[]): TickPoint[] {
  if (prev === undefined || prev.length === 0) return incoming.slice(-MAX_POINTS)
  const byKey = new Map<string, TickPoint>()
  for (const p of prev) byKey.set(pointKey(p), p)
  for (const p of incoming) byKey.set(pointKey(p), p)
  return [...byKey.values()].sort((a, b) => a.atMs - b.atMs).slice(-MAX_POINTS)
}

const EMPTY: MetricsWs = {
  status: 'connecting',
  overview: null,
  ticks: null,
  live: null,
  lastFrameAtMs: null,
  frames: 0,
  dropped: 0,
  reconnects: 0,
}

// Live `/ws/dashboard` state: `hello` is a full replacement, `snapshot` replaces
// and extends the rolling series. Survives a half-open socket without help.
//
// `enabled` is the tenancy gate: until the workspace knows this process hosts
// its game, no socket is opened, so one game's page can never stream another
// game's numbers. A disabled socket reports `closed`, not `connecting`.
export function useMetricsWs(gameId: string | null, enabled: boolean): MetricsWs {
  const [view, setView] = useState<MetricsWs>(EMPTY)

  useEffect(() => {
    if (!enabled) {
      setView({ ...EMPTY, status: 'closed' })
      return
    }

    // A new scope starts blank: carrying the previous game's overview across
    // would show its numbers under this workspace's header for a moment.
    setView(EMPTY)

    let disposed = false
    let socket: WebSocket | null = null
    let attempt = 0
    let frames = 0
    let dropped = 0
    let reconnects = 0
    let lastFrameAt = 0
    let sawError = false
    let reconnectTimer: number | undefined
    let watchdogTimer: number | undefined

    const commit = (to: (prev: MetricsWs) => MetricsWs) => {
      if (disposed) return
      setView((prev) => to({ ...prev, frames, dropped, reconnects }))
    }

    const publish = (patch: Partial<MetricsWs>) => commit((prev) => ({ ...prev, ...patch }))

    const clearWatchdog = () => {
      if (watchdogTimer !== undefined) window.clearTimeout(watchdogTimer)
      watchdogTimer = undefined
    }

    const armWatchdog = () => {
      clearWatchdog()
      watchdogTimer = window.setTimeout(() => {
        watchdogTimer = undefined
        if (!disposed && socket !== null && Date.now() - lastFrameAt >= WATCHDOG_MS) {
          abandon()
          scheduleReconnect()
        }
      }, WATCHDOG_MS)
    }

    const backoffMs = () => {
      const raw = Math.min(MIN_BACKOFF_MS * 2 ** attempt, MAX_BACKOFF_MS)
      // Full jitter: a page reloading into a restart storm must not reconnect in lockstep.
      return raw / 2 + Math.random() * (raw / 2)
    }

    const scheduleReconnect = () => {
      if (disposed || reconnectTimer !== undefined) return
      attempt += 1
      reconnectTimer = window.setTimeout(() => {
        reconnectTimer = undefined
        reconnects += 1
        connect()
      }, backoffMs())
    }

    const abandon = () => {
      if (socket === null) return
      const dead = socket
      socket = null
      dead.onopen = null
      dead.onmessage = null
      dead.onerror = null
      dead.onclose = null
      dead.close()
    }

    const apply = (frame: WsFrame) => {
      attempt = 0
      sawError = false
      frames += 1
      lastFrameAt = Date.now()
      armWatchdog()
      if (frame.type === 'hello') {
        publish({ status: 'open', overview: frame.overview, ticks: frame.ticks, lastFrameAtMs: lastFrameAt })
        return
      }
      if (frame.type === 'snapshot') {
        commit((prev) => ({
          ...prev,
          status: 'open',
          overview: frame.overview,
          ticks:
            prev.ticks === null
              ? frame.ticks
              : { ...frame.ticks, points: mergePoints(prev.ticks.points, frame.ticks.points) },
          lastFrameAtMs: lastFrameAt,
        }))
        return
      }
      if (frame.type === 'live') {
        commit((prev) => ({ ...prev, live: { topic: frame.topic, seq: frame.seq } }))
        return
      }
      // A graceful plane-side close is still a close: say so, then come back.
      abandon()
      clearWatchdog()
      publish({ status: 'closed' })
      scheduleReconnect()
    }

    const connect = () => {
      if (disposed) return
      let ws: WebSocket
      try {
        ws = new WebSocket(wsUrl(gameId))
      } catch {
        publish({ status: 'error' })
        scheduleReconnect()
        return
      }
      socket = ws
      publish({ status: 'connecting' })
      ws.onopen = () => {
        lastFrameAt = Date.now()
        armWatchdog()
        publish({ status: 'open' })
      }
      ws.onmessage = (event: MessageEvent) => {
        const parsed = parseFrame(typeof event.data === 'string' ? event.data : '')
        if (parsed.kind === 'noise') {
          dropped += 1
          publish({})
          return
        }
        if (parsed.kind === 'protocol') {
          // Retrying reads the same unsupported hello, so stop and say so.
          dropped += 1
          clearWatchdog()
          abandon()
          commit((prev) => ({ ...prev, status: 'error', overview: null, ticks: null }))
          return
        }
        apply(parsed.frame)
      }
      ws.onerror = () => {
        sawError = true
      }
      ws.onclose = () => {
        if (socket === ws) socket = null
        clearWatchdog()
        publish({ status: sawError ? 'error' : 'closed' })
        scheduleReconnect()
      }
    }

    connect()

    return () => {
      disposed = true
      if (reconnectTimer !== undefined) window.clearTimeout(reconnectTimer)
      clearWatchdog()
      abandon()
    }
  }, [gameId, enabled])

  return view
}
