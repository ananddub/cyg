import { useState } from "react"
import logoUrl from "@/assets/logo.png"
import {
  Activity,
  Layers,
  Users,
  Cpu,
  ShieldAlert,
  Server,
} from "lucide-react"

import { BusHealthCard } from "@/components/bus-health"
import { ClientsTable } from "@/components/clients-table"
import { DashboardHeader } from "@/components/dashboard-header"
import { LimitsPanel } from "@/components/limits-panel"
import { OfflineBanner } from "@/components/offline-banner"
import { RoomDetailsView } from "@/components/room-details-view"
import { RoomsTable } from "@/components/rooms-table"
import { StatCards } from "@/components/stat-cards"
import { TickChart } from "@/components/tick-chart"
import { TickHistogram } from "@/components/tick-histogram"
import { TransportMixPanel } from "@/components/transport-mix"
import { WorkersStrip } from "@/components/workers-strip"
import { useDashboardData } from "@/hooks/use-dashboard-api"
import { useDashboardToasts } from "@/hooks/use-dashboard-toasts"
import type { RoomRow } from "@/lib/types"
import { cn } from "@/lib/utils"

type NavSection = "overview" | "rooms" | "clients" | "workers" | "limits"

interface NavItem {
  id: NavSection
  label: string
  icon: typeof Activity
  badge?: string | number
}

export function DashboardPage() {
  const data = useDashboardData()
  useDashboardToasts(data.ws, data.overview, data.rooms)

  const [activeSection, setActiveSection] = useState<NavSection>("overview")
  const [selectedRoom, setSelectedRoom] = useState<RoomRow | null>(null)

  const { overview, ticks, ws } = data
  const loading = overview === null
  const offline = ws.status !== "open" && overview === null

  const navItems: NavItem[] = [
    {
      id: "overview",
      label: "Overview & Realtime",
      icon: Activity,
    },
    {
      id: "rooms",
      label: "Game Rooms",
      icon: Layers,
      badge: overview?.rooms ?? 0,
    },
    {
      id: "clients",
      label: "Connected Clients",
      icon: Users,
      badge: overview?.ccu ?? 0,
    },
    {
      id: "workers",
      label: "Simulation Cores",
      icon: Cpu,
      badge: data.workers?.length ?? 2,
    },
    {
      id: "limits",
      label: "Limits & Guardrails",
      icon: ShieldAlert,
    },
  ]

  return (
    <div className="flex min-h-svh w-full bg-background text-foreground">
      {/* ── Left Modern Sidebar (Separated by border-r from top to bottom) ── */}
      <aside className="sticky top-0 hidden h-svh w-64 shrink-0 flex-col border-r border-border/60 bg-card/40 backdrop-blur-md md:flex">
        {/* Brand / Logo - Box 1 at Top-Left */}
        <div className="flex h-16 w-full items-center justify-center px-4 border-b border-border/60">
          <img
            src={logoUrl}
            alt="cookyourgame (cfg)"
            className="h-[52px] w-auto max-w-full object-contain drop-shadow-sm transition-transform duration-200 hover:scale-105"
          />
        </div>

        {/* Navigation Links */}
        <nav className="flex-1 space-y-1 px-3 py-4">
          <div className="px-2 pb-2 text-[10px] font-semibold tracking-wider text-muted-foreground/70 uppercase">
            Navigation
          </div>
          {navItems.map((item) => {
            const Icon = item.icon
            const isActive = activeSection === item.id
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => setActiveSection(item.id)}
                className={cn(
                  "group flex w-full items-center justify-between rounded-sm px-3 py-2 text-xs font-medium transition-all",
                  isActive
                    ? "bg-primary/10 text-primary shadow-xs ring-1 ring-primary/20"
                    : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
                )}
              >
                <div className="flex items-center gap-2.5">
                  <Icon
                    className={cn(
                      "size-4 transition-colors",
                      isActive ? "text-primary" : "text-muted-foreground group-hover:text-foreground"
                    )}
                  />
                  <span>{item.label}</span>
                </div>
                {item.badge !== undefined && (
                  <span
                    className={cn(
                      "font-mono text-xs tabular-nums transition-colors",
                      isActive
                        ? "font-semibold text-primary"
                        : "text-muted-foreground/70 group-hover:text-muted-foreground"
                    )}
                  >
                    {item.badge}
                  </span>
                )}
              </button>
            )
          })}
        </nav>

        {/* Live Engine Status Mini-Widget - Bottom Placed */}
        <div className="mx-3 mb-3 rounded-md border border-border/50 bg-background/50 p-3 shadow-xs">
          <div className="flex items-center justify-between text-xs">
            <span className="text-muted-foreground">Authoritative Tick</span>
            <span className="inline-flex items-center gap-1.5 font-mono text-primary font-medium">
              <span className="size-1.5 rounded-full bg-primary animate-ping" />
              128 Hz
            </span>
          </div>
          <div className="mt-2 flex items-center justify-between text-xs font-mono">
            <span className="text-muted-foreground">Cadence / dt</span>
            <span className="text-foreground font-medium">7.81 ms</span>
          </div>
          {overview?.server && (
            <div className="mt-2 border-t border-border/40 pt-2 flex items-center justify-between text-[11px] text-muted-foreground">
              <span>Room: {overview.server.name}</span>
              <span className="font-mono">:{overview.server.controlPort}</span>
            </div>
          )}
        </div>

        {/* Sidebar Footer */}
        <div className="border-t border-border/50 p-3">
          <div className="flex items-center justify-between rounded-sm bg-background/40 px-3 py-2 text-xs text-muted-foreground">
            <div className="flex items-center gap-2">
              <Server className="size-3.5 text-muted-foreground" />
              <span className="font-mono text-[11px]">v0.2.0-hotpath</span>
            </div>
            <span className="text-[10px] text-primary/80 font-medium">Ready</span>
          </div>
        </div>
      </aside>

      {/* ── Main Scrollable Area ── */}
      <div className="flex min-w-0 flex-1 flex-col">
        {/* Top Header Bar - Separated from sidebar by vertical border */}
        <div className="sticky top-0 z-10 flex h-16 items-center border-b border-border/60 bg-background/80 px-4 backdrop-blur-md lg:px-8">
          <DashboardHeader
            overview={overview ?? undefined}
            source={data.source}
            connected={ws.status === "open"}
            lastFrameAtMs={ws.lastFrameAtMs ?? undefined}
            sectionLabel={navItems.find((n) => n.id === activeSection)?.label}
            roomName={selectedRoom?.name}
          />
        </div>

        {/* Content Body */}
        <main className="flex-1 space-y-6 p-4 lg:p-8">
          {offline && <OfflineBanner lastGoodAtMs={Date.now()} />}

          {/* Dynamic Active Section Views */}
          {activeSection === "overview" && (
            <div className="flex flex-col gap-6">
              <div className="-mx-4 -mt-4 mb-2 border-b border-border/60 px-4 py-6 lg:-mx-8 lg:-mt-8 lg:px-8">
                <h1 className="font-heading text-2xl font-bold tracking-tight text-foreground">
                  Overview & Realtime
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                  Real-time authoritative simulation telemetry, tick rate distribution, and network transport mix.
                </p>
              </div>

              <StatCards overview={overview ?? undefined} />

              <div className="grid gap-6 lg:grid-cols-2">
                <TickChart
                  series={ticks ?? undefined}
                  nominalUs={overview?.tick.nominalUs}
                  loading={loading}
                />
                <div className="rounded-md border border-border/60 bg-card/30 p-5 shadow-xs">
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
                <TransportMixPanel
                  transport={overview?.transport}
                  capacity={overview?.capacity}
                  loading={loading}
                />
                <BusHealthCard bus={overview?.bus} loading={loading} />
              </div>
            </div>
          )}

          {activeSection === "rooms" && (
            selectedRoom ? (
              <RoomDetailsView
                room={selectedRoom}
                clients={data.clients ?? undefined}
                onBack={() => setSelectedRoom(null)}
              />
            ) : (
              <div className="space-y-6">
                <div className="-mx-4 -mt-4 mb-2 border-b border-border/60 px-4 py-6 lg:-mx-8 lg:-mt-8 lg:px-8">
                  <h1 className="font-heading text-2xl font-bold tracking-tight text-foreground">
                    Active Game Rooms
                  </h1>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Authoritative WASM room instances running on Simulation Plane. Click any room to inspect active players.
                  </p>
                </div>

                <div className="rounded-md border border-border/60 bg-card/30 overflow-hidden shadow-xs">
                  <RoomsTable
                    rooms={data.rooms ?? undefined}
                    clients={data.clients ?? undefined}
                    loading={data.rooms === null}
                    onSelectRoom={(room) => setSelectedRoom(room)}
                  />
                </div>
              </div>
            )
          )}

          {activeSection === "clients" && (
            <div className="space-y-6">
              <div className="-mx-4 -mt-4 mb-2 border-b border-border/60 px-4 py-6 lg:-mx-8 lg:-mt-8 lg:px-8">
                <h1 className="font-heading text-2xl font-bold tracking-tight text-foreground">
                  Connected Clients
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                  Live network transport sessions sorted by last input packet silence and sequence acknowledgements.
                </p>
              </div>

              <div className="rounded-md border border-border/60 bg-card/30 overflow-hidden shadow-xs">
                <ClientsTable clients={data.clients ?? undefined} loading={data.clients === null} />
              </div>
            </div>
          )}

          {activeSection === "workers" && (
            <div className="space-y-6">
              <div className="-mx-4 -mt-4 mb-2 border-b border-border/60 px-4 py-6 lg:-mx-8 lg:-mt-8 lg:px-8">
                <h1 className="font-heading text-2xl font-bold tracking-tight text-foreground">
                  Simulation Cores
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                  Dedicated worker threads executing tick loops across CPU cores.
                </p>
              </div>

              <WorkersStrip workers={data.workers ?? undefined} loading={data.workers === null} />
            </div>
          )}

          {activeSection === "limits" && (
            <div className="space-y-6">
              <div className="-mx-4 -mt-4 mb-2 border-b border-border/60 px-4 py-6 lg:-mx-8 lg:-mt-8 lg:px-8">
                <h1 className="font-heading text-2xl font-bold tracking-tight text-foreground">
                  Limits & Guardrails
                </h1>
                <p className="mt-1 text-sm text-muted-foreground">
                  Seat reservation timeouts, token bucket rate limiting and Agones fleet integration.
                </p>
              </div>

              <LimitsPanel
                seats={overview?.seats}
                rateLimit={overview?.rateLimit}
                agones={overview?.agones}
                loading={loading}
              />
            </div>
          )}
        </main>
      </div>
    </div>
  )
}
