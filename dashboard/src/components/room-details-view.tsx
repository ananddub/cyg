import { useState, useMemo } from "react"
import {
  ArrowLeft,
  Users,
  Activity,
  ArrowUpDown,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Card } from "@/components/ui/card"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip"
import type { RoomRow, ClientRow } from "@/lib/types"
import { cn } from "@/lib/utils"

interface RoomDetailsViewProps {
  room: RoomRow
  clients?: ClientRow[] | undefined
  onBack: () => void
}

const TRANSPORT_VARIANT: Record<string, "secondary" | "outline" | "ghost"> = {
  websocket: "secondary",
  webrtc: "outline",
  "native-udp": "ghost",
}

function formatIdle(ms: number | null | undefined): string {
  if (typeof ms !== "number") return "—"
  if (ms < 1000) return `${Math.round(ms)} ms`
  const s = ms / 1000
  if (s < 60) return `${s.toFixed(1)} s`
  return `${Math.floor(s / 60)}m ${Math.floor(s % 60)}s`
}

function idleClass(ms: number | null | undefined): string {
  if (typeof ms !== "number") return "text-muted-foreground"
  if (ms > 2000) return "text-destructive font-bold"
  if (ms > 500) return "text-warning"
  return "text-primary font-medium"
}

export function RoomDetailsView({ room, clients, onBack }: RoomDetailsViewProps) {
  const [sortField, setSortField] = useState<"id" | "idle" | "seq">("id")
  const [sortAsc, setSortAsc] = useState(true)

  // Filter clients belonging to this room
  const roomClients = useMemo(() => {
    const list = (clients ?? []).filter((c) => c.room === room.name || !c.room)
    return [...list].sort((a, b) => {
      if (sortField === "id") {
        const idA = a.playerId ?? 9999
        const idB = b.playerId ?? 9999
        return sortAsc ? idA - idB : idB - idA
      }
      if (sortField === "idle") {
        const idleA = a.idleMs ?? 999999
        const idleB = b.idleMs ?? 999999
        return sortAsc ? idleA - idleB : idleB - idleA
      }
      const seqA = a.ackedSeq ?? 0
      const seqB = b.ackedSeq ?? 0
      return sortAsc ? seqA - seqB : seqB - seqA
    })
  }, [clients, room.name, sortField, sortAsc])

  const toggleSort = (field: "id" | "idle" | "seq") => {
    if (sortField === field) {
      setSortAsc(!sortAsc)
    } else {
      setSortField(field)
      setSortAsc(true)
    }
  }

  return (
    <div className="flex flex-col gap-5">
      {/* ── Room Page Title Area (Matching test.png: Clean, unboxed typography with full separator) ── */}
      <div className="-mx-4 -mt-4 mb-2 border-b border-border/60 px-4 py-6 lg:-mx-8 lg:-mt-8 lg:px-8 flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={onBack}
              title="Back to Rooms"
              aria-label="Back to rooms"
              className="group inline-flex items-center justify-center p-1 -ml-1 text-muted-foreground hover:text-foreground transition-colors hover:bg-foreground/5 rounded-xs"
            >
              <ArrowLeft className="size-5 transition-transform group-hover:-translate-x-0.5" />
            </button>

            <h1 className="font-heading text-2xl font-bold tracking-tight text-foreground">
              {room.name}
            </h1>

            <Badge
              variant="default"
              className={cn(
                "font-mono text-[10px] tracking-wide uppercase px-2 py-0.5",
                room.status === "live"
                  ? "bg-primary/20 text-primary ring-1 ring-primary/30 border-0"
                  : "bg-muted text-muted-foreground"
              )}
            >
              {room.status}
            </Badge>
          </div>
          <p className="mt-1 text-sm text-muted-foreground">
            Authoritative room instance running at {room.tickRate} Hz on UDP socket :{room.port} (Signaling :{room.controlPort}).
          </p>
        </div>

        <div className="flex items-center gap-3">
          <div className="hidden sm:flex items-center gap-4 text-xs font-mono text-muted-foreground">
            <div className="flex items-center gap-1.5">
              <Activity className="size-3.5 text-primary" />
              <span>Tick: <strong className="text-foreground">{room.tickRate} Hz</strong></span>
            </div>
            <div className="flex items-center gap-1.5">
              <Users className="size-3.5 text-primary" />
              <span>CCU: <strong className="text-foreground">{roomClients.length}</strong> / {room.maxClients}</span>
            </div>
          </div>
        </div>
      </div>

      {/* ── Room Key Metrics Summary Cards ── */}
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <Card className="p-4">
          <span className="text-xs font-medium text-muted-foreground">Connected Players</span>
          <div className="mt-1 flex items-baseline gap-2">
            <span className="font-mono text-2xl font-bold text-foreground">{roomClients.length}</span>
            <span className="text-xs text-muted-foreground">/ {room.maxClients} cap</span>
          </div>
        </Card>

        <Card className="p-4">
          <span className="text-xs font-medium text-muted-foreground">Authoritative Rate</span>
          <div className="mt-1 flex items-baseline gap-2">
            <span className="font-mono text-2xl font-bold text-primary">128.0 Hz</span>
            <span className="text-xs text-muted-foreground">7.81 ms dt</span>
          </div>
        </Card>

        <Card className="p-4">
          <span className="text-xs font-medium text-muted-foreground">Simulation UDP Port</span>
          <div className="mt-1 flex items-baseline gap-2">
            <span className="font-mono text-2xl font-bold text-foreground">:{room.port}</span>
            <span className="text-xs text-muted-foreground">Raw Socket</span>
          </div>
        </Card>

        <Card className="p-4">
          <span className="text-xs font-medium text-muted-foreground">Signaling Gateway</span>
          <div className="mt-1 flex items-baseline gap-2">
            <span className="font-mono text-2xl font-bold text-foreground">:{room.controlPort}</span>
            <span className="text-xs text-muted-foreground">WebRTC / WS</span>
          </div>
        </Card>
      </div>

      {/* ── Tabular Connected Users / Players Table ── */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="font-heading text-lg font-semibold tracking-tight text-foreground">
              Active Room Participants
            </h2>
            <p className="text-xs text-muted-foreground">
              Real-time tabular roster of players connected to this room&rsquo;s simulation loop
            </p>
          </div>
          <Badge variant="outline" className="font-mono text-xs">
            {roomClients.length} Connected
          </Badge>
        </div>

        <div className="rounded-lg border border-border bg-card overflow-hidden shadow-xs">
          <TooltipProvider>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead
                    className="w-24 cursor-pointer select-none"
                    onClick={() => toggleSort("id")}
                  >
                    <div className="flex items-center gap-1.5">
                      <span>Player ID</span>
                      <ArrowUpDown className="size-3 text-muted-foreground" />
                    </div>
                  </TableHead>
                  <TableHead>Session ID</TableHead>
                  <TableHead>Transport</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead
                    className="cursor-pointer select-none text-right"
                    onClick={() => toggleSort("seq")}
                  >
                    <div className="flex items-center justify-end gap-1.5">
                      <span>Acked Seq</span>
                      <ArrowUpDown className="size-3 text-muted-foreground" />
                    </div>
                  </TableHead>
                  <TableHead
                    className="cursor-pointer select-none text-right"
                    onClick={() => toggleSort("idle")}
                  >
                    <div className="flex items-center justify-end gap-1.5">
                      <span>Last Seen / Idle</span>
                      <ArrowUpDown className="size-3 text-muted-foreground" />
                    </div>
                  </TableHead>
                  <TableHead className="text-right">Connection Status</TableHead>
                </TableRow>
              </TableHeader>

              <TableBody>
                {roomClients.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={7} className="py-12 text-center text-muted-foreground">
                      <Users className="mx-auto mb-2 size-8 text-muted-foreground/40" />
                      <p className="text-sm font-medium">No players currently connected to &ldquo;{room.name}&rdquo;</p>
                      <p className="mt-1 text-xs text-muted-foreground/70">
                        When clients connect via WebRTC or UDP, their live telemetry will appear here.
                      </p>
                    </TableCell>
                  </TableRow>
                ) : (
                  roomClients.map((client) => {
                    const isSpectator = client.playerId === null || client.playerId === undefined
                    return (
                      <TableRow key={client.session} className="font-mono text-xs">
                        {/* Player ID */}
                        <TableCell>
                          <div className="flex items-center gap-2">
                            <span className="flex size-6 items-center justify-center rounded-xs bg-primary/10 font-bold text-primary ring-1 ring-primary/20">
                              #{client.playerId ?? "—"}
                            </span>
                          </div>
                        </TableCell>

                        {/* Session ID */}
                        <TableCell>
                          <Tooltip>
                            <TooltipTrigger render={<span className="font-mono text-xs text-foreground cursor-default underline decoration-dotted underline-offset-4" />}>
                              {client.session}
                            </TooltipTrigger>
                            <TooltipContent>
                              <p className="font-mono text-xs">Full Colyseus Session: {client.session}</p>
                            </TooltipContent>
                          </Tooltip>
                        </TableCell>

                        {/* Transport */}
                        <TableCell>
                          <Badge
                            variant={TRANSPORT_VARIANT[client.transport] ?? "outline"}
                            className="font-mono text-[10px] uppercase font-bold tracking-wider"
                          >
                            {client.transport}
                          </Badge>
                        </TableCell>

                        {/* Role */}
                        <TableCell>
                          <span
                            className={cn(
                              "font-sans text-xs font-medium",
                              isSpectator ? "text-muted-foreground" : "text-foreground"
                            )}
                          >
                            {isSpectator ? "Observer / Spectator" : "Authoritative Player"}
                          </span>
                        </TableCell>

                        {/* Acked Seq */}
                        <TableCell className="text-right font-mono tabular-nums">
                          {typeof client.ackedSeq === "number" ? client.ackedSeq : "—"}
                        </TableCell>

                        {/* Idle / Last Input */}
                        <TableCell className={cn("text-right font-mono tabular-nums", idleClass(client.idleMs))}>
                          {formatIdle(client.idleMs)}
                        </TableCell>

                        {/* Live Status Pill */}
                        <TableCell className="text-right">
                          <span className="inline-flex items-center gap-1.5 rounded-xs bg-primary/10 px-2 py-0.5 text-[10px] font-sans font-medium text-primary">
                            <span className="size-1.5 rounded-full bg-primary animate-pulse" />
                            Live
                          </span>
                        </TableCell>
                      </TableRow>
                    )
                  })
                )}
              </TableBody>
            </Table>
          </TooltipProvider>
        </div>
      </div>
    </div>
  )
}
