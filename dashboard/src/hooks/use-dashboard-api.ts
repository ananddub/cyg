import { useQuery } from '@tanstack/react-query'

import {
  fetchClients,
  fetchIdentity,
  fetchOverview,
  fetchRooms,
  fetchTicks,
  fetchWorkers,
} from '@/lib/api'
import type { ClientRow, Overview, RoomRow, TickSeries, WorkerRow } from '@/lib/types'
import { useMetricsWs, type MetricsWs } from './use-metrics-ws'

const POLL_MS = 5_000
// A WS frame older than this is a socket that opened and then went quiet; the
// poll is the fresher source, so `useDashboardData` switches to it.
const WS_FRESH_MS = 3_000

const keys = {
  identity: ['dashboard', 'identity'] as const,
  overview: (gameId: string | null) => ['dashboard', 'overview', gameId] as const,
  rooms: (gameId: string | null) => ['dashboard', 'rooms', gameId] as const,
  workers: (gameId: string | null) => ['dashboard', 'workers', gameId] as const,
  clients: (gameId: string | null) => ['dashboard', 'clients', gameId] as const,
  ticks: (gameId: string | null) => ['dashboard', 'ticks', gameId] as const,
}

const poll = { staleTime: POLL_MS, refetchInterval: POLL_MS }

/** The game this process hosts. Read once per page: it is a deployment
 * property, not a live measurement. */
export function useIdentityQuery() {
  return useQuery({
    queryKey: keys.identity,
    queryFn: fetchIdentity,
    staleTime: 30_000,
    retry: 0,
  })
}

export function useOverviewQuery(gameId: string | null, enabled = true) {
  return useQuery({
    queryKey: keys.overview(gameId),
    queryFn: () => fetchOverview(gameId),
    enabled,
    ...poll,
  })
}

export function useRoomsQuery(gameId: string | null, enabled = true) {
  return useQuery({
    queryKey: keys.rooms(gameId),
    queryFn: () => fetchRooms(gameId),
    enabled,
    ...poll,
  })
}

export function useWorkersQuery(gameId: string | null, enabled = true) {
  return useQuery({
    queryKey: keys.workers(gameId),
    queryFn: () => fetchWorkers(gameId),
    enabled,
    ...poll,
  })
}

export function useClientsQuery(gameId: string | null, enabled = true) {
  return useQuery({
    queryKey: keys.clients(gameId),
    queryFn: () => fetchClients(gameId),
    enabled,
    ...poll,
  })
}

export function useTicksQuery(gameId: string | null, enabled = true) {
  return useQuery({
    queryKey: keys.ticks(gameId),
    queryFn: () => fetchTicks(gameId),
    enabled,
    ...poll,
  })
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

/**
 * Live data for one game workspace. `enabled` is false until the workspace has
 * confirmed this process hosts that game; a disabled feed neither polls nor
 * opens a socket, so another tenant's workspace cannot accidentally render
 * this process's numbers.
 */
export function useDashboardData(gameId: string | null, enabled: boolean): DashboardData {
  const ws = useMetricsWs(gameId, enabled)
  const overview = useOverviewQuery(gameId, enabled)
  const ticks = useTicksQuery(gameId, enabled)
  const rooms = useRoomsQuery(gameId, enabled)
  const workers = useWorkersQuery(gameId, enabled)
  const clients = useClientsQuery(gameId, enabled)

  if (!enabled) {
    return {
      source: 'poll',
      overview: null,
      ticks: null,
      rooms: null,
      workers: null,
      clients: null,
      isLoading: false,
      ws,
    }
  }

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
