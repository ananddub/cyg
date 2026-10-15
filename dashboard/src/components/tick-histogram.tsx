import { cn } from "@/lib/utils"
import { Skeleton } from "@/components/ui/skeleton"

interface TickHistogramProps {
  histogram: number[] | undefined
  edgesUs: number[] | undefined
  nominalUs: number | undefined
  samples: number | undefined
  loading: boolean
}

const BUCKETS = 12

// Bucket `i` covers `[2^i, 2^(i+1))` µs and the last absorbs everything longer,
// so its edge is only a floor; the server ships the edges but a missing array
// must still draw a legible axis.
function edgeAt(edges: number[] | undefined, i: number): number {
  return edges?.[i] ?? 2 ** (i + 1)
}

// ms above 1000 µs (2 decimals), integer µs below — the dashboard-wide rule.
function formatEdge(us: number): string {
  return us >= 1000 ? `${(us / 1000).toFixed(2)} ms` : `${us} µs`
}

// The open-ended top bin is named by what it swallows, not a fake ceiling.
function trimMs(us: number): string {
  return `>= ${Number((us / 1000).toFixed(3)).toString()} ms`
}

// Green guarantees the whole bin is under the period budget; amber within 2x;
// the catch-all bin is always alarming because an overrun hides in it.
function tintFor(i: number, last: number, edge: number, nominal: number | undefined): string {
  if (nominal == null || !Number.isFinite(nominal)) return "bg-muted-foreground/50"
  if (i === last) return "bg-destructive"
  if (edge <= nominal) return "bg-chart-2"
  if (edge <= nominal * 2) return "bg-warning"
  return "bg-destructive"
}

export function TickHistogram({ histogram, edgesUs, nominalUs, samples, loading }: TickHistogramProps) {
  if (loading) {
    return <Skeleton className="h-44 w-full" />
  }

  const counts = histogram ?? []
  const n = Math.max(counts.length, edgesUs?.length ?? 0) || BUCKETS
  const last = n - 1
  const hasNominal = nominalUs != null && Number.isFinite(nominalUs)
  const total = counts.reduce((sum, c) => sum + (c ?? 0), 0)
  const hasData = total > 0 || (samples ?? 0) > 0
  const yMax = Math.max(...counts.map((c) => c ?? 0), 1)

  return (
    <div className="flex flex-col gap-3">
      <div className="flex gap-2">
        <div className="flex h-40 w-10 shrink-0 flex-col justify-between text-right font-mono text-[10px] text-muted-foreground tabular-nums">
          <span>{yMax}</span>
          <span>0</span>
        </div>

        <div className="relative h-40 flex-1">
          <div className="absolute inset-0 flex flex-col justify-between">
            {[0, 1, 2, 3].map((g) => (
              <div key={g} className="border-t border-border/40" />
            ))}
          </div>

          <div className="absolute inset-0 flex items-end gap-1">
            {Array.from({ length: n }, (_, i) => {
              const count = counts[i] ?? 0
              const edge = edgeAt(edgesUs, i)
              const heightPct = (count / yMax) * 100
              const label = i === last ? trimMs(edge) : formatEdge(edge)
              return (
                <div
                  key={i}
                  className="flex h-full flex-1 items-end"
                  title={`${i === last ? label : `≤ ${label}`}: ${count} tick${count === 1 ? "" : "s"}`}
                >
                  <div
                    className={cn("w-full rounded-t-sm transition-[height] duration-300", tintFor(i, last, edge, nominalUs))}
                    style={{ height: `${hasData ? Math.max(heightPct, count > 0 ? 2 : 0) : 0}%` }}
                  />
                </div>
              )
            })}
          </div>

          {!hasData && (
            <div className="absolute inset-0 flex items-center justify-center">
              <span className="text-xs text-muted-foreground">No data yet</span>
            </div>
          )}
        </div>
      </div>

      <div className="flex gap-2">
        <div className="w-10 shrink-0" />
        <div className="flex flex-1 gap-1">
          {Array.from({ length: n }, (_, i) => {
            const edge = edgeAt(edgesUs, i)
            return (
              <div key={i} className="min-w-0 flex-1 truncate text-center font-mono text-[9px] text-muted-foreground">
                {i === last ? trimMs(edge) : formatEdge(edge)}
              </div>
            )
          })}
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-muted-foreground">
        <span className="inline-flex items-center gap-1.5">
          <span className={cn("h-2 w-2 rounded-[2px]", hasNominal ? "bg-chart-2" : "bg-muted-foreground/50")} />
          {hasNominal ? "≤ nominal" : "no budget"}
        </span>
        <span className="inline-flex items-center gap-1.5">
          <span className="h-2 w-2 rounded-[2px] bg-warning" /> ≤ 2× nominal
        </span>
        <span className="inline-flex items-center gap-1.5">
          <span className="h-2 w-2 rounded-[2px] bg-destructive" /> overrun / catch-all
        </span>
      </div>
    </div>
  )
}
