// The runtime-monitoring areas, now rendered inside the console shell. They read
// the one `useDashboardData` result the shell owns — the metrics socket must be
// opened once, not once per view.

import { useState } from "react"

import { BusHealthCard } from "@/components/bus-health"
import { ClientsTable } from "@/components/clients-table"
import { PageHeading } from "@/components/console/data-table"
import { LimitsPanel } from "@/components/limits-panel"
import { RoomDetailsView } from "@/components/room-details-view"
import { RoomsTable } from "@/components/rooms-table"
import { StatCards } from "@/components/stat-cards"
import { TickChart } from "@/components/tick-chart"
import { TickHistogram } from "@/components/tick-histogram"
import { TransportMixPanel } from "@/components/transport-mix"
import { WorkersStrip } from "@/components/workers-strip"
import type { DashboardData } from "@/hooks/use-dashboard-api"
import type { WorkerRow } from "@/lib/types"

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

      <div className="grid gap-6 lg:grid-cols-2">
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

      <div className="grid gap-6 lg:grid-cols-2">
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

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Game Rooms"
        description="Authoritative room instances running on the simulation plane. Select a room to inspect its players."
      />
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xs">
        <RoomsTable
          rooms={data.rooms ?? undefined}
          clients={data.clients ?? undefined}
          loading={data.rooms === null}
          onSelectRoom={(row) => setSelected(row.name)}
        />
      </div>
    </div>
  )
}

export function ClientsView({ data }: { data: DashboardData }) {
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Connected Clients"
        description="Live network transport sessions, sorted by input silence and sequence acknowledgements."
      />
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xs">
        <ClientsTable clients={data.clients ?? undefined} loading={data.clients === null} />
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
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Simulation Cores"
        description="Dedicated worker threads executing tick loops across CPU cores."
      />
      <WorkersStrip
        workers={data.workers ?? undefined}
        loading={data.workers === null}
        onSelectWorker={onSelectWorker}
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
