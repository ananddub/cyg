// The runtime-monitoring areas, now rendered inside the console shell. They read
// the one `useDashboardData` result the shell owns — the metrics socket must be
// opened once, not once per view.

import { useState } from "react"

import { BusHealthCard } from "@/components/bus-health"
import { ClientsTable } from "@/components/clients-table"
import { FilterPills, PageHeading, SearchField } from "@/components/console/data-table"
import { LimitsPanel } from "@/components/limits-panel"
import { RoomDetailsView } from "@/components/room-details-view"
import { RoomsTable } from "@/components/rooms-table"
import { StatCards } from "@/components/stat-cards"
import { TickChart } from "@/components/tick-chart"
import { TickHistogram } from "@/components/tick-histogram"
import { TransportMixPanel } from "@/components/transport-mix"
import { WorkersStrip } from "@/components/workers-strip"
import type { DashboardData } from "@/hooks/use-dashboard-api"
import type { ClientTransport, RoomStatus, WorkerRow } from "@/lib/types"

type RoomFilter = "all" | RoomStatus
type TransportFilter = "all" | ClientTransport

const ROOM_FILTERS: { id: RoomFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "live", label: "Live" },
  { id: "empty", label: "Empty" },
]

const TRANSPORT_FILTERS: { id: TransportFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "websocket", label: "WebSocket" },
  { id: "webrtc", label: "WebRTC" },
  { id: "native-udp", label: "UDP" },
]

export function RuntimeView({ data }: { data: DashboardData }) {
  const { overview, ticks, ws } = data
  const loading = overview === null

  return (
    <div className="flex flex-col gap-6">
      <PageHeading
        title="Overview & Realtime"
        description="Authoritative simulation telemetry, tick rate distribution and network transport mix."
      />

      <StatCards overview={overview ?? undefined} />

      <div className="stagger-in grid gap-6 lg:grid-cols-2">
        <TickChart series={ticks ?? undefined} nominalUs={overview?.tick.nominalUs} loading={loading} />
        <div className="rounded-lg border border-border bg-card p-5 shadow-xs">
          <div className="mb-4">
            <h3 className="font-heading text-base font-semibold text-foreground">Tick distribution</h3>
            <p className="text-xs text-muted-foreground">Execution time per base-2 µs bucket</p>
          </div>
          <TickHistogram
            histogram={ticks?.histogram}
            edgesUs={ticks?.bucketEdgesUs}
            nominalUs={overview?.tick.nominalUs}
            samples={overview?.tick.samples}
            loading={loading}
          />
        </div>
      </div>

      <div className="stagger-in grid gap-6 lg:grid-cols-2">
        <TransportMixPanel transport={overview?.transport} capacity={overview?.capacity} loading={loading} />
        <BusHealthCard bus={overview?.bus} loading={loading} />
      </div>

      <p className="text-[11px] text-muted-foreground/70">
        Feed: {ws.status === "open" ? "live socket" : `REST poll (socket ${ws.status})`}
        {ws.reconnects > 0 && ` · ${ws.reconnects} reconnects`}
      </p>
    </div>
  )
}

export function RoomsView({ data }: { data: DashboardData }) {
  const [selected, setSelected] = useState<string | null>(null)
  const [query, setQuery] = useState("")
  const [filter, setFilter] = useState<RoomFilter>("all")
  const room = selected === null ? null : (data.rooms ?? []).find((candidate) => candidate.name === selected) ?? null

  if (room !== null) {
    return (
      <RoomDetailsView
        room={room}
        clients={data.clients ?? undefined}
        onBack={() => setSelected(null)}
      />
    )
  }

  const needle = query.trim().toLowerCase()
  const rooms = (data.rooms ?? []).filter((row) => {
    if (filter !== "all" && row.status !== filter) return false
    if (needle === "") return true
    return [row.name, row.serverId, row.signalingUrl, row.status].some((value) =>
      value.toLowerCase().includes(needle),
    )
  })

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Game Rooms"
        description="Authoritative room instances running on the simulation plane. Select a room to inspect its players."
      />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <SearchField
          value={query}
          onChange={setQuery}
          placeholder="Search rooms, servers or signaling URLs"
          label="Search rooms"
          className="sm:flex-1"
        />
        <FilterPills
          filters={ROOM_FILTERS}
          value={filter}
          onChange={setFilter}
          className="sm:ml-auto"
        />
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xs">
        <RoomsTable
          rooms={data.rooms === null ? undefined : rooms}
          clients={data.clients ?? undefined}
          loading={data.rooms === null}
          onSelectRoom={(row) => setSelected(row.name)}
          empty={
            (data.rooms?.length ?? 0) > 0
              ? "No rooms match these filters."
              : "No rooms active."
          }
        />
      </div>
    </div>
  )
}

export function ClientsView({ data }: { data: DashboardData }) {
  const [query, setQuery] = useState("")
  const [filter, setFilter] = useState<TransportFilter>("all")

  const needle = query.trim().toLowerCase()
  const clients = (data.clients ?? []).filter((row) => {
    if (filter !== "all" && row.transport !== filter) return false
    if (needle === "") return true
    return [row.session, row.playerId === null ? "" : String(row.playerId), row.transport, row.room].some(
      (value) => value.toLowerCase().includes(needle),
    )
  })

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Connected Clients"
        description="Live network transport sessions, sorted by input silence and sequence acknowledgements."
      />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <SearchField
          value={query}
          onChange={setQuery}
          placeholder="Search sessions, players, rooms or transports"
          label="Search clients"
          className="sm:flex-1"
        />
        <FilterPills
          filters={TRANSPORT_FILTERS}
          value={filter}
          onChange={setFilter}
          className="sm:ml-auto"
        />
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xs">
        <ClientsTable
          clients={data.clients === null ? undefined : clients}
          loading={data.clients === null}
          empty={
            (data.clients?.length ?? 0) > 0
              ? "No clients match these filters."
              : "No clients connected."
          }
        />
      </div>
    </div>
  )
}

export function CoresView({
  data,
  onSelectWorker,
}: {
  data: DashboardData
  onSelectWorker: (worker: WorkerRow) => void
}) {
  const [query, setQuery] = useState("")
  const needle = query.trim().toLowerCase()
  const workers = (data.workers ?? []).filter(
    (worker) =>
      needle === "" ||
      [worker.name, worker.kind, worker.live ? "live" : "idle"].some((value) =>
        value.toLowerCase().includes(needle),
      ),
  )

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Simulation Cores"
        description="Dedicated worker threads executing tick loops across CPU cores."
      />

      <SearchField
        value={query}
        onChange={setQuery}
        placeholder="Search cores by name, plane or status"
        label="Search simulation cores"
      />

      <WorkersStrip
        workers={data.workers === null ? undefined : workers}
        loading={data.workers === null}
        onSelectWorker={onSelectWorker}
        empty={
          (data.workers?.length ?? 0) > 0
            ? "No cores match this search."
            : "No simulation cores reported yet."
        }
      />
    </div>
  )
}

export function LimitsView({ data }: { data: DashboardData }) {
  const { overview } = data
  const loading = overview === null
  return (
    <div className="space-y-6">
      <PageHeading
        title="Limits & Guardrails"
        description="Seat reservation timeouts, token-bucket rate limiting and Agones fleet integration."
      />
      <LimitsPanel
        seats={overview?.seats}
        rateLimit={overview?.rateLimit}
        agones={overview?.agones}
        loading={loading}
      />
    </div>
  )
}
