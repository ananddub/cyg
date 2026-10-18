// Core telemetry: the console's one non-list drawer. It rides the same Sheet as
// every detail surface, so Escape, the scrim and the focus trap behave alike.
// The parent keeps holding the selected worker; closing means the parent
// clearing it, which flips `open` false.

import {
  Activity,
  AlertTriangle,
  Clock,
  Cpu,
  Gauge,
  Layers,
  Radio,
  Send,
  ShieldCheck,
  Users,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import type { WorkerRow, Overview } from "@/lib/types"
import { cn } from "@/lib/utils"

interface CoreDetailsSidebarProps {
  worker: WorkerRow | null
  overview?: Overview | null
  onClose: () => void
}

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

function Stat({
  label,
  value,
  note,
  tone,
}: {
  label: string
  value: string
  note: string
  tone?: "primary" | "destructive"
}) {
  return (
    <div className="rounded-md border border-border/70 bg-card p-3 shadow-2xs">
      <span className="text-xs text-muted-foreground">{label}</span>
      <div
        className={cn(
          "mt-1 font-mono text-base font-bold",
          tone === "primary" && "text-primary",
          tone === "destructive" && "text-destructive",
          tone === undefined && "text-foreground"
        )}
      >
        {value}
      </div>
      <span className="text-[11px] text-muted-foreground">{note}</span>
    </div>
  )
}

function PipelineRow({
  icon: Icon,
  label,
  value,
  alarm,
}: {
  icon: typeof Users
  label: string
  value: string
  alarm?: boolean
}) {
  return (
    <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
      <div className="flex items-center gap-2 text-muted-foreground">
        <Icon className={cn("size-3.5", alarm ? "text-warning" : "text-primary")} />
        <span>{label}</span>
      </div>
      <span className={cn("font-mono font-medium", alarm ? "font-bold text-destructive" : "text-foreground")}>
        {value}
      </span>
    </div>
  )
}

function SpecRow({ label, value, muted }: { label: string; value: string; muted?: boolean }) {
  return (
    <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <span className={cn("font-mono font-medium", muted ? "text-muted-foreground" : "text-foreground")}>
        {value}
      </span>
    </div>
  )
}

export function CoreDetailsSidebar({ worker, overview, onClose }: CoreDetailsSidebarProps) {
  // Budget calculations for simulation worker
  const nominalUs = worker?.nominalTickUs ?? (overview?.tick.nominalUs || 7812)
  const p99Us = worker?.tickP99Us ?? 0
  const budgetUsedPct = nominalUs > 0 ? Math.min(100, Math.max(0.1, (p99Us / nominalUs) * 100)) : 0

  const isSim = worker?.name === "sim"
  const isControl = worker?.name === "control"

  return (
    <Sheet open={worker !== null} onOpenChange={(next) => !next && onClose()}>
      <SheetContent side="right" className="gap-0">
        {worker !== null && (
          <>
            <SheetHeader className="pr-14 text-left">
              <div className="flex items-center gap-3">
                <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/25">
                  <Cpu className="size-5" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <SheetTitle>Core: {worker.name.toUpperCase()}</SheetTitle>
                    <Badge variant={kindVariant(worker.kind)}>{worker.kind}</Badge>
                  </div>
                  <SheetDescription className="text-xs">
                    {isSim
                      ? "Authoritative Game Simulation Plane"
                      : isControl
                        ? "Tokio Async Control & Signaling Plane"
                        : "Socket Reactor & Transport Worker"}
                  </SheetDescription>
                </div>
                <div className="flex shrink-0 items-center gap-1.5 font-mono text-xs font-medium text-primary">
                  <span className={cn("size-2 rounded-full bg-primary", worker.live && "animate-pulse")} />
                  <span>{worker.live ? "LIVE" : "IDLE"}</span>
                </div>
              </div>
            </SheetHeader>

            <div className="flex-1 space-y-6 overflow-y-auto px-6 py-4 text-sm">
              {/* Plane Summary Banner */}
              <div className="rounded-md border border-border/80 bg-background/60 p-3.5 shadow-xs">
                <div className="flex items-start gap-2.5">
                  <Activity className="size-4 shrink-0 text-primary mt-0.5" />
                  <div className="text-xs leading-relaxed text-muted-foreground">
                    {isSim && (
                      <span>
                        Executing <strong className="text-foreground">128 Hz</strong> authoritative
                        simulation loop pinned to dedicated CPU core. Zero-allocation WASM memory
                        ensures sub-microsecond determinism.
                      </span>
                    )}
                    {isControl && (
                      <span>
                        Multi-threaded Tokio async reactor handling WebRTC SDP signaling, HTTP control
                        plane endpoints, and room lifecycle orchestration.
                      </span>
                    )}
                    {!isSim && !isControl && (
                      <span>
                        Dedicated socket worker processing high-frequency packet I/O, sequence
                        acknowledgments, and backpressure filtering.
                      </span>
                    )}
                  </div>
                </div>
              </div>

              {/* Real-time Latency & Budget Diagnostics */}
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <h3 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    <Gauge className="size-3.5 text-primary" />
                    <span>Execution & Latency Profile</span>
                  </h3>
                  {isSim && (
                    <span className="font-mono text-[11px] font-medium text-primary">
                      {overview?.tick.hz ? `${overview.tick.hz.toFixed(1)} Hz Authoritative` : "128.0 Hz"}
                    </span>
                  )}
                </div>

                <div className="grid grid-cols-2 gap-2.5">
                  <Stat
                    label="Nominal Budget"
                    value={worker.nominalTickUs ? formatDuration(worker.nominalTickUs) : "Event Loop"}
                    note={worker.nominalTickUs ? "7.81 ms per tick" : "Asynchronous reactor"}
                  />
                  <Stat
                    label="Tick Median (p50)"
                    value={formatDuration(worker.tickP50Us)}
                    note="50th percentile runtime"
                  />
                  <Stat
                    label="Tick Peak (p99)"
                    value={formatDuration(worker.tickP99Us)}
                    note="99th percentile worst-case"
                    tone="primary"
                  />
                  <Stat
                    label="Overruns"
                    value={formatCount(worker.overruns)}
                    note="Ticks exceeding budget"
                    tone={(worker.overruns ?? 0) > 0 ? "destructive" : undefined}
                  />
                </div>

                {isSim && (
                  <div className="space-y-2 rounded-md border border-border/70 bg-card p-3.5 shadow-2xs">
                    <div className="flex items-center justify-between text-xs">
                      <span className="text-muted-foreground">CPU Budget Utilization</span>
                      <span className="font-mono text-sm font-semibold text-primary">
                        {budgetUsedPct.toFixed(2)}% consumed
                      </span>
                    </div>
                    <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
                      <div
                        className="h-full rounded-full bg-primary transition-all duration-300"
                        style={{ width: `${budgetUsedPct}%` }}
                      />
                    </div>
                    <div className="flex items-center justify-between font-mono text-[11px] text-muted-foreground">
                      <span>p99: {formatDuration(p99Us)}</span>
                      <span>Headroom: {(100 - budgetUsedPct).toFixed(1)}% free</span>
                    </div>
                  </div>
                )}
              </div>

              {/* Workload & I/O Pipeline */}
              <div className="space-y-3">
                <h3 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  <Radio className="size-3.5 text-primary" />
                  <span>Workload & Delta Pipeline</span>
                </h3>

                <div className="divide-y divide-border/60 rounded-md border border-border/70 bg-card shadow-2xs">
                  <PipelineRow icon={Users} label="Connected Clients (CCU)" value={formatCount(worker.ccu)} />
                  <PipelineRow icon={Send} label="Deltas Sent" value={formatCount(worker.deltasSent)} />
                  <PipelineRow
                    icon={AlertTriangle}
                    label="Deltas Dropped"
                    value={formatCount(worker.deltasDropped)}
                    alarm={(worker.deltasDropped ?? 0) > 0}
                  />
                  <PipelineRow icon={Layers} label="Inbound Queue Depth" value={formatCount(worker.queueDepth)} />
                </div>
              </div>

              {/* Architecture & OS Specifications */}
              <div className="space-y-3">
                <h3 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  <ShieldCheck className="size-3.5 text-primary" />
                  <span>Architecture & Isolation</span>
                </h3>

                <div className="divide-y divide-border/60 rounded-md border border-border/70 bg-card shadow-2xs">
                  <SpecRow
                    label="OS Worker Threads"
                    value={`${worker.threads} Dedicated Thread${worker.threads > 1 ? "s" : ""}`}
                  />
                  <SpecRow
                    label="Scheduling Model"
                    value={worker.kind === "game" ? "Thread-Per-Core (Pinned)" : "Tokio Work-Stealing"}
                  />
                  <SpecRow label="Memory Architecture" value="Zero-Copy Shared Linear Memory" muted />
                  <SpecRow label="Host ABI Contract" value="game_host_v1" />
                  <SpecRow label="Telemetry Sequence" value={`#${overview?.tick.seq ?? DASH}`} muted />
                </div>
              </div>
            </div>

            <SheetFooter>
              <div className="flex items-center justify-between text-xs text-muted-foreground">
                <div className="flex items-center gap-1.5">
                  <Clock className="size-3.5" />
                  <span>Telemetry sample age:</span>
                </div>
                <span className="font-mono font-medium text-foreground">
                  {overview?.sampleAgeMs != null ? `${overview.sampleAgeMs} ms` : "Live"}
                </span>
              </div>
            </SheetFooter>
          </>
        )}
      </SheetContent>
    </Sheet>
  )
}
