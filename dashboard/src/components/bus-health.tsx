import { useEffect, useRef, useState } from "react"
import { Skeleton } from "@/components/ui/skeleton"
import type { BusHealth } from "@/lib/types"
import { cn } from "cn"

const DASH = "—"

// Queue capacities are compile-time constants of the bus, not on the wire:
// inputs/deltas caps come from `Bus::new(4096, 8192)` in src/main.rs, the
// commands cap is the fixed `bounded(256)` in engine-bus. BusHealth carries no
// cap field, so saturation thresholds hard-code them here.
const CAPS = { inputs: 4096, commands: 256, deltas: 8192 } as const

function formatCount(value: number | null | undefined): string {
  if (value === null || value === undefined) return DASH
  return value.toLocaleString()
}

/** -1 falling, 0 flat, 1 rising; undefined until a second reading exists. */
type Trend = -1 | 0 | 1

interface Queues {
  inputs: number
  commands: number
  deltas: number
}

function useQueueTrend(bus: BusHealth | undefined): Record<keyof Queues, Trend> | undefined {
  const prev = useRef<BusHealth | undefined>(undefined)
  const [trend, setTrend] = useState<Record<keyof Queues, Trend> | undefined>(undefined)
  useEffect(() => {
    if (!bus) return
    const p = prev.current
    if (p) {
      const sign = (a: number, b: number): Trend => (a > b ? 1 : a < b ? -1 : 0)
      setTrend({
        inputs: sign(bus.inputsQueued, p.inputsQueued),
        commands: sign(bus.commandsQueued, p.commandsQueued),
        deltas: sign(bus.deltasQueued, p.deltasQueued),
      })
    }
    prev.current = bus
  }, [bus])
  return trend
}

function arrow(trend: Trend | undefined): string {
  if (trend === undefined) return ""
  return trend > 0 ? "↑" : trend < 0 ? "↓" : "→"
}

function QueueTile({
  label,
  value,
  cap,
  trend,
}: {
  label: string
  value: number | undefined
  cap: number
  trend: Trend | undefined
}) {
  const v = value ?? 0
  const full = value !== undefined && v >= cap
  const warn = value !== undefined && !full && v > cap / 2
  return (
    <div
      className={cn(
        "flex flex-col gap-1 rounded-md border bg-card p-3 shadow-xs",
        full ? "border-destructive" : warn ? "border-warning" : "border-border"
      )}
    >
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="flex items-baseline gap-1.5">
        <span
          className={cn(
            "tabular-nums text-2xl font-medium leading-none",
            full ? "text-destructive" : warn ? "text-warning" : "text-foreground"
          )}
        >
          {value === undefined ? DASH : formatCount(value)}
        </span>
        {trend !== undefined && value !== undefined ? (
          <span
            title={trend > 0 ? "growing" : trend < 0 ? "draining" : "steady"}
            className="text-sm text-muted-foreground"
          >
            {arrow(trend)}
          </span>
        ) : null}
      </span>
      <span className="text-xs text-muted-foreground">cap {cap.toLocaleString()}</span>
    </div>
  )
}

function DropCounter({ label, value, danger }: { label: string; value: number | undefined; danger: boolean }) {
  return (
    <div className="flex items-baseline justify-between gap-2 border-border py-1">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className={cn("tabular-nums text-sm", danger ? "text-destructive font-medium" : "text-foreground")}>
        {formatCount(value)}
      </span>
    </div>
  )
}

export function BusHealthCard({
  bus,
  loading,
}: {
  bus: BusHealth | undefined
  loading: boolean
}) {
  const trend = useQueueTrend(bus)

  if (loading) {
    return (
      <div className="flex flex-col gap-3">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="h-24 rounded-md" />
          ))}
        </div>
        <Skeleton className="h-24 rounded-md" />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <QueueTile label="inputs queued" value={bus?.inputsQueued} cap={CAPS.inputs} trend={trend?.inputs} />
        <QueueTile label="commands queued" value={bus?.commandsQueued} cap={CAPS.commands} trend={trend?.commands} />
        <QueueTile label="deltas queued" value={bus?.deltasQueued} cap={CAPS.deltas} trend={trend?.deltas} />
      </div>

      <div className="flex flex-col rounded-md border border-border bg-card p-3">
        <DropCounter label="deltas sent" value={bus?.deltasSent} danger={false} />
        <DropCounter
          label="deltas dropped (full)"
          value={bus?.deltasDroppedFull}
          danger={(bus?.deltasDroppedFull ?? 0) > 0}
        />
        <DropCounter
          label="inputs dropped"
          value={bus?.inputsDropped}
          danger={(bus?.inputsDropped ?? 0) > 0}
        />
      </div>
    </div>
  )
}
