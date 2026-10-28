import type { ReactNode } from "react"

import { Card, CardContent } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import type { Overview } from "@/lib/types"
import { cn } from "@/lib/utils"

const EM_DASH = "—"

function formatUs(us: number | null | undefined): string {
  if (us == null || !Number.isFinite(us)) return EM_DASH
  // The API speaks µs; only cross a millisecond once the number earns it.
  if (us > 1000) return `${(us / 1000).toFixed(2)} ms`
  return `${Math.round(us)} µs`
}

function formatUptime(sec: number | null | undefined): string {
  if (sec == null || !Number.isFinite(sec)) return EM_DASH
  const d = Math.floor(sec / 86400)
  const h = Math.floor((sec % 86400) / 3600)
  const m = Math.floor((sec % 3600) / 60)
  const s = Math.floor(sec % 60)
  if (d > 0) return `${d}d ${h}h`
  if (h > 0) return `${h}h ${m}m`
  if (m > 0) return `${m}m ${s}s`
  return `${s}s`
}

type Tone = "default" | "warning" | "destructive"

const toneClass: Record<Tone, string> = {
  default: "text-foreground",
  warning: "text-warning",
  destructive: "text-destructive",
}

interface StatCardProps {
  label: string
  /** Pass null/undefined to render the em dash; `loading` renders a skeleton. */
  value: ReactNode
  secondary: ReactNode
  tone?: Tone
  loading?: boolean
}

function StatCard({ label, value, secondary, tone = "default", loading }: StatCardProps) {
  return (
    <Card size="sm">
      <CardContent>
        <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          {label}
        </p>
        {loading ? (
          <>
            <Skeleton className="mt-1.5 h-7 w-16" />
            <Skeleton className="mt-1.5 h-3.5 w-24" />
          </>
        ) : (
          <>
            <p className={cn("mt-1 text-2xl leading-none font-semibold tabular-nums", toneClass[tone])}>
              {value}
            </p>
            <p className="mt-1.5 truncate text-xs text-muted-foreground">{secondary}</p>
          </>
        )}
      </CardContent>
    </Card>
  )
}

export function StatCards({ overview }: { overview?: Overview }) {
  if (!overview) {
    return (
      <div className="stagger-in grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6">
        {Array.from({ length: 6 }, (_, i) => (
          <StatCard key={i} label="" value="" secondary="" loading />
        ))}
      </div>
    )
  }

  const tick = overview.tick
  // Zero tick fields with hasSimSample false mean "never published", not a
  // healthy idle sim — an em dash is the only honest rendering.
  const sampled = overview.hasSimSample === true
  const drift =
    sampled && tick.configuredHz > 0 &&
    Math.abs(tick.hz - tick.configuredHz) > 0.05 * tick.configuredHz
  const dropped = overview.bus.deltasDroppedFull

  return (
    <div className="stagger-in grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6">
      <StatCard
        label="CCU"
        value={overview.ccu.toLocaleString()}
        secondary={`${overview.capacity.players} / ${overview.capacity.max} players`}
      />
      <StatCard label="Rooms" value={overview.rooms} secondary={overview.server.name || EM_DASH} />
      <StatCard
        label="Tick rate"
        tone={drift ? "warning" : "default"}
        value={sampled ? `${tick.hz.toFixed(1)} Hz` : EM_DASH}
        secondary={`configured ${tick.configuredHz || EM_DASH} Hz`}
      />
      <StatCard
        label="Tick p99"
        value={sampled ? formatUs(tick.p99Us) : EM_DASH}
        secondary={sampled ? `budget ${formatUs(tick.nominalUs)}` : "no sample"}
      />
      <StatCard
        label="Sim stage"
        value={sampled ? formatUs(tick.stageSimUs) : EM_DASH}
        secondary={sampled ? "µs/tick in room ticks" : "no sample"}
      />
      <StatCard label="Uptime" value={formatUptime(overview.uptimeSec)} secondary="since process start" />
      <StatCard
        label="Dropped deltas"
        tone={dropped > 0 ? "destructive" : "default"}
        value={dropped.toLocaleString()}
        secondary={`${overview.bus.deltasSent.toLocaleString()} sent`}
      />
    </div>
  )
}
