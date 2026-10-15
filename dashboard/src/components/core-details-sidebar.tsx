import { useEffect } from "react"
import {
  X,
  Cpu,
  Activity,
  Layers,
  ShieldCheck,
  Clock,
  Gauge,
  Radio,
  Users,
  Send,
  AlertTriangle,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
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

export function CoreDetailsSidebar({
  worker,
  overview,
  onClose,
}: CoreDetailsSidebarProps) {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose()
    }
    window.addEventListener("keydown", handleKeyDown)
    return () => window.removeEventListener("keydown", handleKeyDown)
  }, [onClose])

  if (!worker) return null

  const isSim = worker.name === "sim"
  const isControl = worker.name === "control"

  // Budget calculations for simulation worker
  const nominalUs = worker.nominalTickUs ?? (overview?.tick.nominalUs || 7812)
  const p99Us = worker.tickP99Us ?? 0
  const budgetUsedPct = nominalUs > 0 ? Math.min(100, Math.max(0.1, (p99Us / nominalUs) * 100)) : 0

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-50 bg-background/80 backdrop-blur-xs transition-opacity animate-in fade-in duration-200"
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Right Slide-Over Sidebar Panel */}
      <aside
        className="fixed inset-y-0 right-0 z-50 flex w-full max-w-md sm:max-w-lg flex-col border-l border-border bg-card shadow-2xl transition-all duration-300 animate-in slide-in-from-right"
        role="dialog"
        aria-modal="true"
        aria-label="Simulation Core Telemetry Details"
      >
        {/* Header */}
        <div className="flex items-center justify-between border-b border-border/70 px-5 py-4">
          <div className="flex items-center gap-3">
            <div className="flex size-9 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/25">
              <Cpu className="size-5" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h2 className="font-heading text-lg font-bold text-foreground">
                  Core: {worker.name.toUpperCase()}
                </h2>
                <Badge variant={kindVariant(worker.kind)}>{worker.kind}</Badge>
              </div>
              <p className="text-xs text-muted-foreground">
                {isSim
                  ? "Authoritative Game Simulation Plane"
                  : isControl
                    ? "Tokio Async Control & Signaling Plane"
                    : "Socket Reactor & Transport Worker"}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-3">
            <div className="flex items-center gap-1.5 text-xs font-mono font-medium text-primary">
              <span className="size-2 rounded-full bg-primary animate-pulse" />
              <span>{worker.live ? "LIVE" : "IDLE"}</span>
            </div>
            <button
              type="button"
              onClick={onClose}
              className="inline-flex size-8 items-center justify-center rounded-xs text-muted-foreground hover:bg-foreground/5 hover:text-foreground transition-colors"
              title="Close panel"
            >
              <X className="size-4" />
            </button>
          </div>
        </div>

        {/* Scrollable Content */}
        <div className="flex-1 space-y-6 overflow-y-auto p-5 text-sm">
          {/* Plane Summary Banner */}
          <div className="rounded-md border border-border/80 bg-background/60 p-3.5 shadow-xs">
            <div className="flex items-start gap-2.5">
              <Activity className="size-4 text-primary shrink-0 mt-0.5" />
              <div className="text-xs leading-relaxed text-muted-foreground">
                {isSim && (
                  <span>
                    Executing <strong className="text-foreground">128 Hz</strong> authoritative simulation loop pinned to dedicated CPU core. Zero-allocation WASM memory ensures sub-microsecond determinism.
                  </span>
                )}
                {isControl && (
                  <span>
                    Multi-threaded Tokio async reactor handling WebRTC SDP signaling, HTTP control plane endpoints, and room lifecycle orchestration.
                  </span>
                )}
                {!isSim && !isControl && (
                  <span>
                    Dedicated socket worker processing high-frequency packet I/O, sequence acknowledgments, and backpressure filtering.
                  </span>
                )}
              </div>
            </div>
          </div>

          {/* Real-time Latency & Budget Diagnostics */}
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <h3 className="text-xs font-semibold tracking-wider uppercase text-muted-foreground flex items-center gap-1.5">
                <Gauge className="size-3.5 text-primary" />
                <span>Execution & Latency Profile</span>
              </h3>
              {isSim && (
                <span className="text-[11px] font-mono text-primary font-medium">
                  {overview?.tick.hz ? `${overview.tick.hz.toFixed(1)} Hz Authoritative` : "128.0 Hz"}
                </span>
              )}
            </div>

            <div className="grid grid-cols-2 gap-2.5">
              <div className="rounded-md border border-border/70 bg-card p-3 shadow-2xs">
                <span className="text-xs text-muted-foreground">Nominal Budget</span>
                <div className="mt-1 font-mono text-base font-bold text-foreground">
                  {worker.nominalTickUs ? formatDuration(worker.nominalTickUs) : "Event Loop"}
                </div>
                <span className="text-[11px] text-muted-foreground">
                  {worker.nominalTickUs ? "7.81 ms per tick" : "Asynchronous reactor"}
                </span>
              </div>

              <div className="rounded-md border border-border/70 bg-card p-3 shadow-2xs">
                <span className="text-xs text-muted-foreground">Tick Median (p50)</span>
                <div className="mt-1 font-mono text-base font-bold text-foreground">
                  {formatDuration(worker.tickP50Us)}
                </div>
                <span className="text-[11px] text-muted-foreground">50th percentile runtime</span>
              </div>

              <div className="rounded-md border border-border/70 bg-card p-3 shadow-2xs">
                <span className="text-xs text-muted-foreground">Tick Peak (p99)</span>
                <div className="mt-1 font-mono text-base font-bold text-primary">
                  {formatDuration(worker.tickP99Us)}
                </div>
                <span className="text-[11px] text-muted-foreground">99th percentile worst-case</span>
              </div>

              <div className="rounded-md border border-border/70 bg-card p-3 shadow-2xs">
                <span className="text-xs text-muted-foreground">Overruns</span>
                <div
                  className={cn(
                    "mt-1 font-mono text-base font-bold",
                    (worker.overruns ?? 0) > 0 ? "text-destructive" : "text-foreground"
                  )}
                >
                  {formatCount(worker.overruns)}
                </div>
                <span className="text-[11px] text-muted-foreground">Ticks exceeding budget</span>
              </div>
            </div>

            {/* Budget Utilization Gauge for Simulation Worker */}
            {isSim && (
              <div className="rounded-md border border-border/70 bg-card p-3.5 shadow-2xs space-y-2">
                <div className="flex items-center justify-between text-xs">
                  <span className="text-muted-foreground">CPU Budget Utilization</span>
                  <span className="font-mono text-primary font-semibold">
                    {budgetUsedPct.toFixed(2)}% consumed
                  </span>
                </div>
                <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full bg-primary transition-all duration-300"
                    style={{ width: `${budgetUsedPct}%` }}
                  />
                </div>
                <div className="flex items-center justify-between text-[11px] text-muted-foreground font-mono">
                  <span>p99: {formatDuration(p99Us)}</span>
                  <span>Headroom: {(100 - budgetUsedPct).toFixed(1)}% free</span>
                </div>
              </div>
            )}
          </div>

          {/* Workload & I/O Pipeline */}
          <div className="space-y-3">
            <h3 className="text-xs font-semibold tracking-wider uppercase text-muted-foreground flex items-center gap-1.5">
              <Radio className="size-3.5 text-primary" />
              <span>Workload & Delta Pipeline</span>
            </h3>

            <div className="rounded-md border border-border/70 bg-card divide-y divide-border/60 shadow-2xs">
              <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
                <div className="flex items-center gap-2 text-muted-foreground">
                  <Users className="size-3.5 text-primary" />
                  <span>Connected Clients (CCU)</span>
                </div>
                <span className="font-mono font-medium text-foreground">
                  {formatCount(worker.ccu)}
                </span>
              </div>

              <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
                <div className="flex items-center gap-2 text-muted-foreground">
                  <Send className="size-3.5 text-primary" />
                  <span>Deltas Sent</span>
                </div>
                <span className="font-mono font-medium text-foreground">
                  {formatCount(worker.deltasSent)}
                </span>
              </div>

              <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
                <div className="flex items-center gap-2 text-muted-foreground">
                  <AlertTriangle className="size-3.5 text-warning" />
                  <span>Deltas Dropped</span>
                </div>
                <span
                  className={cn(
                    "font-mono font-medium",
                    (worker.deltasDropped ?? 0) > 0 ? "text-destructive font-bold" : "text-foreground"
                  )}
                >
                  {formatCount(worker.deltasDropped)}
                </span>
              </div>

              <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
                <div className="flex items-center gap-2 text-muted-foreground">
                  <Layers className="size-3.5 text-primary" />
                  <span>Inbound Queue Depth</span>
                </div>
                <span className="font-mono font-medium text-foreground">
                  {formatCount(worker.queueDepth)}
                </span>
              </div>
            </div>
          </div>

          {/* Architecture & OS Specifications */}
          <div className="space-y-3">
            <h3 className="text-xs font-semibold tracking-wider uppercase text-muted-foreground flex items-center gap-1.5">
              <ShieldCheck className="size-3.5 text-primary" />
              <span>Architecture & Isolation</span>
            </h3>

            <div className="rounded-md border border-border/70 bg-card divide-y divide-border/60 shadow-2xs">
              <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
                <span className="text-muted-foreground">OS Worker Threads</span>
                <span className="font-mono font-medium text-foreground">
                  {worker.threads} Dedicated Thread{worker.threads > 1 ? "s" : ""}
                </span>
              </div>

              <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
                <span className="text-muted-foreground">Scheduling Model</span>
                <span className="font-mono font-medium text-primary">
                  {worker.kind === "game" ? "Thread-Per-Core (Pinned)" : "Tokio Work-Stealing"}
                </span>
              </div>

              <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
                <span className="text-muted-foreground">Memory Architecture</span>
                <span className="font-mono text-muted-foreground">
                  Zero-Copy Shared Linear Memory
                </span>
              </div>

              <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
                <span className="text-muted-foreground">Host ABI Contract</span>
                <span className="font-mono text-foreground font-medium">game_host_v1</span>
              </div>

              <div className="flex items-center justify-between px-3.5 py-2.5 text-xs">
                <span className="text-muted-foreground">Telemetry Sequence</span>
                <span className="font-mono text-muted-foreground">
                  #{overview?.tick.seq ?? DASH}
                </span>
              </div>
            </div>
          </div>
        </div>

        {/* Footer */}
        <div className="border-t border-border/70 bg-background/40 p-4">
          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <div className="flex items-center gap-1.5">
              <Clock className="size-3.5" />
              <span>Telemetry sample age:</span>
            </div>
            <span className="font-mono text-foreground font-medium">
              {overview?.sampleAgeMs != null ? `${overview.sampleAgeMs} ms` : "Live"}
            </span>
          </div>
        </div>
      </aside>
    </>
  )
}
