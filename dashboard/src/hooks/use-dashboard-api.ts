import { useQuery } from '@tanstack/react-query'

import { fetchClients, fetchOverview, fetchRooms, fetchTicks, fetchWorkers } from '@/lib/api'
import type { ClientRow, Overview, RoomRow, TickSeries, WorkerRow } from '@/lib/types'
import { useMetricsWs, type MetricsWs } from './use-metrics-ws'

const POLL_MS = 5_000
// A WS frame older than this is a socket that opened and then went quiet; the
// poll is the fresher source, so `useDashboardData` switches to it.
const WS_FRESH_MS = 3_000

const keys = {
  overview: ['dashboard', 'overview'] as const,
  rooms: ['dashboard', 'rooms'] as const,
  workers: ['dashboard', 'workers'] as const,
  clients: ['dashboard', 'clients'] as const,
  ticks: ['dashboard', 'ticks'] as const,
}

const poll = { staleTime: POLL_MS, refetchInterval: POLL_MS }

export function useOverviewQuery() {
  return useQuery({ queryKey: keys.overview, queryFn: fetchOverview, ...poll })
}

export function useRoomsQuery() {
  return useQuery({ queryKey: keys.rooms, queryFn: fetchRooms, ...poll })
}

export function useWorkersQuery() {
  return useQuery({ queryKey: keys.workers, queryFn: fetchWorkers, ...poll })
}

export function useClientsQuery() {
  return useQuery({ queryKey: keys.clients, queryFn: fetchClients, ...poll })
}

export function useTicksQuery() {
  return useQuery({ queryKey: keys.ticks, queryFn: fetchTicks, ...poll })
}

// Everything the first screen shows, from whichever feed is currently fresher.
// A component reading this never has to know whether a socket or a poll wrote it.
// `null` means "this feed has never resolved" — distinct from an empty array,
// which is a real answer (no rooms / no clients) the tables must render.
export interface DashboardData {
  source: 'ws' | 'poll'
  overview: Overview | null
  ticks: TickSeries | null
  rooms: RoomRow[] | null
  workers: WorkerRow[] | null
  clients: ClientRow[] | null
  // `true` while neither feed has produced an overview yet.
  isLoading: boolean
  // The raw socket view, for a status dot or a reconnect counter.
  ws: MetricsWs
}

export function useDashboardData(): DashboardData {
  const ws = useMetricsWs()
  const overview = useOverviewQuery()
  const ticks = useTicksQuery()
  const rooms = useRoomsQuery()
  const workers = useWorkersQuery()
  const clients = useClientsQuery()

  const fresh =
    ws.overview !== null && ws.lastFrameAtMs !== null && Date.now() - ws.lastFrameAtMs < WS_FRESH_MS

  return {
    source: fresh ? 'ws' : 'poll',
    overview: fresh ? ws.overview : (overview.data ?? null),
    ticks: fresh ? ws.ticks : (ticks.data ?? null),
    rooms: rooms.data ?? null,
    workers: workers.data ?? null,
    clients: clients.data ?? null,
    isLoading: !fresh && overview.data === undefined,
    ws,
  }
}
