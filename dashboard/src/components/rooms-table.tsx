import { useEffect, useRef, useState } from "react"
import { Badge } from "@/components/ui/badge"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { cn } from "@/lib/utils"
import type { RoomRow } from "@/lib/types"

interface RoomsTableProps {
  rooms: RoomRow[] | undefined
  loading: boolean
}

const DASH = "—"

function fillPercent(clients: number, max: number): number {
  if (max <= 0) return 0
  return Math.min(100, Math.round((clients / max) * 100))
}

/** The API gives ms already here; only sub-second ages stay in ms. */
function formatAgeMs(ms: number | null | undefined): string {
  if (ms == null || Number.isNaN(ms)) return DASH
  if (ms < 1000) return `${ms} ms`
  return `${(ms / 1000).toFixed(1)} s`
}

function formatHz(hz: number | null | undefined): string {
  if (hz == null || Number.isNaN(hz)) return DASH
  return hz.toFixed(1)
}

function CopyCell({ value }: { value: string | null | undefined }) {
  const [copied, setCopied] = useState(false)
  const timer = useRef<number | undefined>(undefined)

  useEffect(() => () => window.clearTimeout(timer.current), [])

  if (!value) return <span className="text-muted-foreground">{DASH}</span>

  const copy = () => {
    navigator.clipboard
      ?.writeText(value)
      .then(() => {
        setCopied(true)
        window.clearTimeout(timer.current)
        timer.current = window.setTimeout(() => setCopied(false), 1500)
      })
      .catch(() => setCopied(false))
  }

  return (
    <button
      type="button"
      onClick={copy}
      title={value}
      className={cn(
        "max-w-56 truncate rounded-md border border-border bg-muted/40 px-2 py-1 text-left font-mono text-xs",
        "text-muted-foreground transition-colors hover:bg-muted hover:text-foreground",
      )}
    >
      {copied ? "copied" : value}
    </button>
  )
}

const SKELETON_WIDTHS = ["w-28", "w-24", "w-16", "w-12", "w-10", "w-14", "w-40"] as const

function SkeletonRows() {
  return (
    <>
      {Array.from({ length: 3 }, (_, i) => (
        <TableRow key={`skeleton-${i}`}>
          {SKELETON_WIDTHS.map((w, j) => (
            <TableCell key={j}>
              <Skeleton className={cn("h-4", w)} />
            </TableCell>
          ))}
        </TableRow>
      ))}
    </>
  )
}

export function RoomsTable({ rooms, loading }: RoomsTableProps) {
  const rows = rooms ?? []
  const showSkeletons = loading && rows.length === 0

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Room</TableHead>
          <TableHead>Players</TableHead>
          <TableHead>Tick Hz</TableHead>
          <TableHead>Status</TableHead>
          <TableHead>Empty for</TableHead>
          <TableHead>Sample age</TableHead>
          <TableHead>Signaling URL</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {showSkeletons ? (
          <SkeletonRows />
        ) : (
          rows.map((room) => {
            const live = room.status === "live"
            const pct = fillPercent(room.clients, room.maxClients)
            return (
              <TableRow key={`${room.serverId}/${room.name}`}>
                <TableCell className="font-medium">{room.name}</TableCell>
                <TableCell>
                  <div className="flex items-center gap-2">
                    <div className="h-1.5 w-20 overflow-hidden rounded-full bg-muted">
                      <div
                        className={cn(
                          "h-full rounded-full",
                          live ? "bg-primary" : "bg-muted-foreground/40",
                        )}
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                    <span className="tabular-nums text-xs text-muted-foreground">
                      {room.clients}/{room.maxClients}
                    </span>
                  </div>
                </TableCell>
                <TableCell className="tabular-nums">
                  <span>{room.hasSimSample ? formatHz(room.tickHz) : DASH}</span>
                  <span className="text-muted-foreground"> / {room.tickRate}</span>
                </TableCell>
                <TableCell>
                  <Badge variant={live ? "default" : "secondary"}>{room.status}</Badge>
                </TableCell>
                <TableCell className="tabular-nums">
                  {!live && room.emptyForSec != null ? `${room.emptyForSec}s` : DASH}
                </TableCell>
                <TableCell className="tabular-nums">
                  {formatAgeMs(room.sampleAgeMs)}
                </TableCell>
                <TableCell>
                  <CopyCell value={room.signalingUrl} />
                </TableCell>
              </TableRow>
            )
          })
        )}
        {!showSkeletons && rows.length === 0 && (
          <TableRow>
            <TableCell colSpan={7} className="text-center text-muted-foreground">
              no rooms
            </TableCell>
          </TableRow>
        )}
      </TableBody>
    </Table>
  )
}
