import { Cpu, ArrowRight, Activity, Zap, ShieldCheck } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Card } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import type { WorkerRow } from "@/lib/types"
import { cn } from "@/lib/utils"

const DASH = "—"

function formatDuration(us: number | null | undefined): string {
  if (us === null || us === undefined) return DASH
  return us >= 1000 ? `${(us / 1000).toFixed(2)} ms` : `${us} µs`
}

function formatCount(value: number | null | undefined): string {
  if (value === null || value === undefined) return DASH
  return value.toLocaleString()
}

function kindVariant(kind: string): "default" | "secondary" | "outline" {
  if (kind === "game") return "default"
  if (kind === "tokio") return "secondary"
  return "outline"
}

export interface WorkersStripProps {
  workers: WorkerRow[] | undefined
  loading: boolean
  onSelectWorker?: (worker: WorkerRow) => void
}

export function WorkersStrip({
  workers,
  loading,
  onSelectWorker,
}: WorkersStripProps) {
  if (loading) {
    return (
      <div className="space-y-4">
        <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-24 rounded-md" />
          ))}
        </div>
        <Skeleton className="h-64 rounded-md" />
      </div>
    )
  }

  const rows = workers ?? []
  const simWorker = rows.find((w) => w.name === "sim")
  const totalThreads = rows.reduce((acc, w) => acc + (w.threads || 0), 0)
  const totalOverruns = rows.reduce((acc, w) => acc + (w.overruns || 0), 0)

  return (
    <div className="space-y-6">
      {/* ── KPI Metric Summary Tiles ── */}
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <Card className="p-4 shadow-xs">
          <span className="text-xs font-medium text-muted-foreground flex items-center gap-1.5">
            <Cpu className="size-3.5 text-primary" />
            <span>Simulation Cores</span>
          </span>
          <div className="mt-1 flex items-baseline gap-2">
            <span className="font-mono text-2xl font-bold text-foreground">{rows.length}</span>
            <span className="text-xs text-muted-foreground">active planes</span>
          </div>
        </Card>

        <Card className="p-4 shadow-xs">
          <span className="text-xs font-medium text-muted-foreground flex items-center gap-1.5">
            <Zap className="size-3.5 text-primary" />
            <span>Dedicated Threads</span>
          </span>
          <div className="mt-1 flex items-baseline gap-2">
            <span className="font-mono text-2xl font-bold text-foreground">{totalThreads}</span>
            <span className="text-xs text-muted-foreground">OS worker threads</span>
          </div>
        </Card>

        <Card className="p-4 shadow-xs">
          <span className="text-xs font-medium text-muted-foreground flex items-center gap-1.5">
            <Activity className="size-3.5 text-primary" />
            <span>Peak Tick Latency</span>
          </span>
          <div className="mt-1 flex items-baseline gap-2">
            <span className="font-mono text-2xl font-bold text-primary">
              {formatDuration(simWorker?.tickP99Us)}
            </span>
            <span className="text-xs text-muted-foreground">p99 worst-case</span>
          </div>
        </Card>

        <Card className="p-4 shadow-xs">
          <span className="text-xs font-medium text-muted-foreground flex items-center gap-1.5">
            <ShieldCheck className="size-3.5 text-primary" />
            <span>Total Overruns</span>
          </span>
          <div className="mt-1 flex items-baseline gap-2">
            <span
              className={cn(
                "font-mono text-2xl font-bold",
                totalOverruns > 0 ? "text-foreground" : "text-primary"
              )}
            >
              {totalOverruns}
            </span>
            <span className="text-xs text-muted-foreground">ticks &gt; budget</span>
          </div>
        </Card>
      </div>

      {/* ── Cores Interactive Table List ── */}
      <div className="rounded-lg border border-border bg-card overflow-hidden shadow-xs">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Core / Identifier</TableHead>
              <TableHead>Execution Plane</TableHead>
              <TableHead>Threads</TableHead>
              <TableHead>Latency (p50 / p99)</TableHead>
              <TableHead>Nominal Rate</TableHead>
              <TableHead>Overruns</TableHead>
              <TableHead>CCU / Queues</TableHead>
              <TableHead>Status</TableHead>
              <TableHead className="text-right">Inspect</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.length === 0 ? (
              <TableRow>
                <TableCell colSpan={9} className="text-center text-muted-foreground py-8">
                  No simulation cores reported yet.
                </TableCell>
              </TableRow>
            ) : (
              rows.map((worker) => {
                const isSim = worker.name === "sim"
                const isControl = worker.name === "control"

                return (
                  <TableRow
                    key={worker.name}
                    onClick={() => onSelectWorker?.(worker)}
                    className="group cursor-pointer transition-colors hover:bg-muted/50"
                  >
                    {/* Core Name & Description */}
                    <TableCell className="font-semibold text-foreground">
                      <div className="flex items-center gap-2.5">
                        <div className="flex size-7 items-center justify-center rounded-xs bg-primary/10 text-primary ring-1 ring-primary/25">
                          <Cpu className="size-4" />
                        </div>
                        <div>
                          <span className="font-mono text-xs font-bold uppercase tracking-wider group-hover:text-primary transition-colors">
                            {worker.name}
                          </span>
                          <p className="text-[11px] font-normal text-muted-foreground">
                            {isSim
                              ? "Authoritative WASM Simulation"
                              : isControl
                                ? "Tokio Async I/O & Signaling"
                                : "Transport Socket Reactor"}
                          </p>
                        </div>
                      </div>
                    </TableCell>

                    {/* Execution Plane Badge */}
                    <TableCell>
                      <Badge variant={kindVariant(worker.kind)}>{worker.kind}</Badge>
                    </TableCell>

                    {/* OS Threads */}
                    <TableCell className="tabular-nums font-mono text-xs">
                      <span className="font-semibold text-foreground">{worker.threads}</span>
                      <span className="text-muted-foreground"> thread{worker.threads > 1 ? "s" : ""}</span>
                    </TableCell>

                    {/* Latency: p50 / p99 */}
                    <TableCell className="tabular-nums font-mono text-xs">
                      {worker.tickP99Us != null ? (
                        <>
                          <span className="font-medium text-foreground">
                            {formatDuration(worker.tickP50Us)}
                          </span>
                          <span className="text-muted-foreground"> / </span>
                          <span className="font-bold text-primary">
                            {formatDuration(worker.tickP99Us)}
                          </span>
                        </>
                      ) : (
                        <span className="text-muted-foreground">Event-driven</span>
                      )}
                    </TableCell>

                    {/* Nominal Rate */}
                    <TableCell className="tabular-nums font-mono text-xs">
                      {worker.nominalTickUs ? (
                        <>
                          <span className="font-medium text-foreground">
                            {(1_000_000 / worker.nominalTickUs).toFixed(1)} Hz
                          </span>
                          <span className="text-muted-foreground">
                            {" "}({formatDuration(worker.nominalTickUs)})
                          </span>
                        </>
                      ) : (
                        <span className="text-muted-foreground">Non-ticking</span>
                      )}
                    </TableCell>

                    {/* Overruns */}
                    <TableCell className="tabular-nums font-mono text-xs">
                      <span
                        className={cn(
                          "font-medium",
                          (worker.overruns ?? 0) > 0 ? "text-destructive font-bold" : "text-muted-foreground"
                        )}
                      >
                        {formatCount(worker.overruns)}
                      </span>
                    </TableCell>

                    {/* CCU / Queues */}
                    <TableCell className="tabular-nums font-mono text-xs">
                      <span className="font-medium text-foreground">{formatCount(worker.ccu)} CCU</span>
                      {worker.queueDepth != null && (
                        <span className="text-muted-foreground"> · Q:{worker.queueDepth}</span>
                      )}
                    </TableCell>

                    {/* Status */}
                    <TableCell>
                      <div className="flex items-center gap-1.5 text-xs font-mono font-medium text-primary">
                        <span className="size-1.5 rounded-full bg-primary animate-pulse" />
                        <span>{worker.live ? "LIVE" : "IDLE"}</span>
                      </div>
                    </TableCell>

                    {/* Action Arrow (Unboxed) */}
                    <TableCell className="text-right">
                      <span className="inline-flex items-center justify-end text-muted-foreground transition-all duration-150 group-hover:text-primary group-hover:translate-x-0.5">
                        <ArrowRight className="size-4" />
                      </span>
                    </TableCell>
                  </TableRow>
                )
              })
            )}
          </TableBody>
        </Table>
      </div>
    </div>
  )
}
