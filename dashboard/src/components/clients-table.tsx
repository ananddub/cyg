import { useMemo, useState } from "react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { SelectionCheckbox } from "@/components/console/data-table"
import { useSelection } from "@/hooks/use-selection"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip"
import type { ClientRow } from "@/lib/types"

type Props = {
  clients: ClientRow[] | undefined
  loading: boolean
  /** What to say when the list is empty — a filter or an empty plane. */
  empty?: string
}

type SortDir = "asc" | "desc"

const TRANSPORT_VARIANT: Record<string, "secondary" | "outline" | "ghost"> = {
  websocket: "secondary",
  webrtc: "outline",
  "native-udp": "ghost",
}

function formatIdle(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`
  const s = ms / 1000
  if (s < 60) return `${s.toFixed(1)} s`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m}m ${Math.floor(s % 60)}s`
  return `${Math.floor(m / 60)}h ${m % 60}m`
}

function idleClass(ms: number | null | undefined): string {
  if (typeof ms !== "number") return "text-muted-foreground"
  if (ms > 2000) return "text-destructive"
  if (ms > 500) return "text-warning"
  return "text-foreground"
}

function truncateSession(session: string): string {
  return session.length > 14 ? `…${session.slice(-12)}` : session
}

export function ClientsTable({ clients, loading, empty = "No clients connected" }: Props) {
  const [dir, setDir] = useState<SortDir>("desc")

  const rows = useMemo(() => {
    const list = clients ?? []
    // A session that never sent input has an unknown age; park it at the end
    // in both directions rather than pretend it is the freshest or the stalest.
    const age = (c: ClientRow) => (typeof c.idleMs === "number" ? c.idleMs : null)
    return [...list].sort((a, b) => {
      const av = age(a)
      const bv = age(b)
      if (av === null && bv === null) return a.session.localeCompare(b.session)
      if (av === null) return 1
      if (bv === null) return -1
      return dir === "desc" ? bv - av : av - bv
    })
  }, [clients, dir])

  const selection = useSelection(rows.map((client) => client.session))
  const { selected, allSelected, someSelected } = selection

  return (
    <TooltipProvider>
      {selected.size > 0 && (
        <div className="flex shrink-0 items-center justify-between gap-3 border-b border-border/60 bg-muted/40 px-3 py-1.5 text-xs text-muted-foreground">
          <span className="tabular-nums">{selected.size} selected</span>
          <Button variant="ghost" size="xs" onClick={selection.clear}>
            Clear
          </Button>
        </div>
      )}
      <Table fill>
        <TableHeader className="sticky top-0 z-10 bg-card">
          <TableRow className="hover:bg-transparent">
            <TableHead className="w-8">
              <SelectionCheckbox
                checked={allSelected}
                indeterminate={someSelected}
                onChange={selection.toggleAll}
                label="Select every session on this list"
              />
            </TableHead>
            <TableHead>Session</TableHead>
            <TableHead>Transport</TableHead>
            <TableHead>Player</TableHead>
            <TableHead className="text-right">Acked seq</TableHead>
            <TableHead
              className="text-right cursor-pointer select-none"
              aria-sort={dir === "desc" ? "descending" : "ascending"}
              onClick={() => setDir((d) => (d === "desc" ? "asc" : "desc"))}
            >
              Idle {dir === "desc" ? "↓" : "↑"}
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {loading && clients === undefined ? (
            Array.from({ length: 4 }, (_, i) => (
              <TableRow key={i}>
                {Array.from({ length: 6 }, (_, j) => (
                  <TableCell key={j}>
                    <Skeleton className="h-4 w-full" />
                  </TableCell>
                ))}
              </TableRow>
            ))
          ) : rows.length === 0 ? (
            <TableRow>
              <TableCell colSpan={6} className="py-6 text-center text-muted-foreground">
                {empty}
              </TableCell>
            </TableRow>
          ) : (
            rows.map((c) => (
              <TableRow key={c.session}>
                <TableCell className="w-8">
                  <SelectionCheckbox
                    checked={selected.has(c.session)}
                    onChange={() => selection.toggle(c.session)}
                    label={`Select ${c.session}`}
                  />
                </TableCell>
                <TableCell>
                  <Tooltip>
                    <TooltipTrigger render={<span className="font-mono text-xs" />}>
                      {truncateSession(c.session)}
                    </TooltipTrigger>
                    <TooltipContent>{c.session}</TooltipContent>
                  </Tooltip>
                </TableCell>
                <TableCell>
                  <Badge variant={TRANSPORT_VARIANT[c.transport] ?? "outline"}>{c.transport}</Badge>
                </TableCell>
                <TableCell className={c.playerId === null || c.playerId === undefined ? "text-muted-foreground" : undefined}>
                  {c.playerId === null || c.playerId === undefined ? "spectator" : c.playerId}
                </TableCell>
                <TableCell className="text-right font-mono tabular-nums">
                  {typeof c.ackedSeq === "number" ? c.ackedSeq : "—"}
                </TableCell>
                <TableCell className={`text-right font-mono tabular-nums ${idleClass(c.idleMs)}`}>
                  {typeof c.idleMs === "number" ? formatIdle(c.idleMs) : "—"}
                </TableCell>
              </TableRow>
            ))
          )}
        </TableBody>
      </Table>
    </TooltipProvider>
  )
}
