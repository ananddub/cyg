import { useRef, useEffect, useState } from "react"
import { ArrowRight } from "lucide-react"

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
import type { RoomRow, ClientRow } from "@/lib/types"

interface RoomsTableProps {
  rooms: RoomRow[] | undefined
  clients?: ClientRow[] | undefined
  loading: boolean
  onSelectRoom?: (room: RoomRow) => void
}

const DASH = "—"

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

  const copy = (e: React.MouseEvent) => {
    e.stopPropagation()
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
        "max-w-44 truncate rounded-xs border border-border bg-muted/40 px-2 py-0.5 text-left font-mono text-xs",
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

export function RoomsTable({ rooms, clients, loading, onSelectRoom }: RoomsTableProps) {
  const rows = rooms ?? []
  const showSkeletons = loading && rows.length === 0

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Room</TableHead>
          <TableHead>Players / CCU</TableHead>
          <TableHead>Tick Rate</TableHead>
          <TableHead>Status</TableHead>
          <TableHead>Empty For</TableHead>
          <TableHead>Sample Age</TableHead>
          <TableHead>Signaling URL</TableHead>
          <TableHead className="text-right">Action</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {showSkeletons ? (
          <SkeletonRows />
        ) : (
          rows.map((room) => {
            const live = room.status === "live"
            const roomClientsCount = (clients ?? []).filter((c) => c.room === room.name || !c.room).length

            return (
              <TableRow
                key={`${room.serverId}/${room.name}`}
                onClick={() => onSelectRoom?.(room)}
                className="group cursor-pointer transition-colors hover:bg-muted/50"
              >
                <TableCell className="font-semibold text-foreground">
                  <div className="flex items-center gap-2">
                    <span className="group-hover:text-primary transition-colors">
                      {room.name}
                    </span>
                  </div>
                </TableCell>

                <TableCell className="tabular-nums font-mono text-xs">
                  <span className="font-semibold text-foreground">
                    {roomClientsCount || room.clients}
                  </span>
                  <span className="text-muted-foreground"> / {room.maxClients}</span>
                </TableCell>

                <TableCell className="tabular-nums font-mono">
                  <span className="text-primary font-medium">
                    {room.hasSimSample ? formatHz(room.tickHz) : DASH}
                  </span>
                  <span className="text-muted-foreground"> / {room.tickRate} Hz</span>
                </TableCell>

                <TableCell>
                  <Badge
                    variant={live ? "default" : "secondary"}
                    className={cn(
                      live && "bg-primary/20 text-primary ring-1 ring-primary/30 border-0"
                    )}
                  >
                    {room.status}
                  </Badge>
                </TableCell>

                <TableCell className="tabular-nums font-mono text-xs">
                  {!live && room.emptyForSec != null ? `${room.emptyForSec}s` : DASH}
                </TableCell>

                <TableCell className="tabular-nums font-mono text-xs">
                  {formatAgeMs(room.sampleAgeMs)}
                </TableCell>

                <TableCell>
                  <CopyCell value={room.signalingUrl} />
                </TableCell>

                <TableCell className="text-right">
                  <span className="inline-flex items-center justify-end text-muted-foreground transition-all duration-150 group-hover:text-primary group-hover:translate-x-0.5">
                    <ArrowRight className="size-4" />
                  </span>
                </TableCell>
              </TableRow>
            )
          })
        )}
        {!showSkeletons && rows.length === 0 && (
          <TableRow>
            <TableCell colSpan={8} className="text-center text-muted-foreground py-8">
              No rooms active
            </TableCell>
          </TableRow>
        )}
      </TableBody>
    </Table>
  )
}
