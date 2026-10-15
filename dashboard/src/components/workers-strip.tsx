import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import type { WorkerRow } from "@/lib/types"
import { cn } from "cn"

const DASH = "—"

/** The API speaks microseconds: ms with 2 decimals past 1000 µs, integer µs below. */
function formatDuration(us: number | null | undefined): string {
  if (us === null || us === undefined) return DASH
  return us > 1000 ? `${(us / 1000).toFixed(2)} ms` : `${us} us`
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

function Stat({ label, value, className }: { label: string; value: string; className?: string }) {
  return (
    <div className="flex items-baseline justify-between gap-2">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className={cn("tabular-nums text-sm", className)}>{value}</span>
    </div>
  )
}

function WorkerCard({ worker }: { worker: WorkerRow }) {
  const overruns = worker.overruns
  const dropped = worker.deltasDropped
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>{worker.name}</CardTitle>
        <div className="flex items-center gap-1.5">
          <Badge variant={kindVariant(worker.kind)}>{worker.kind}</Badge>
          {worker.live ? (
            <span
              title="live"
              className="inline-block size-2 shrink-0 animate-pulse rounded-full bg-primary"
            />
          ) : null}
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-1">
        <Stat label="threads" value={formatCount(worker.threads)} />
        <Stat
          label="tick p50"
          value={formatDuration(worker.tickP50Us)}
        />
        <Stat
          label="tick p99"
          value={formatDuration(worker.tickP99Us)}
        />
        <Stat
          label="overruns"
          value={formatCount(overruns)}
          className={(overruns ?? 0) > 0 ? "text-destructive" : undefined}
        />
        <Stat label="ccu" value={formatCount(worker.ccu)} />
        <Stat label="deltas sent" value={formatCount(worker.deltasSent)} />
        <Stat
          label="deltas dropped"
          value={formatCount(dropped)}
          className={(dropped ?? 0) > 0 ? "text-destructive" : undefined}
        />
        <Stat label="queue" value={formatCount(worker.queueDepth)} />
      </CardContent>
    </Card>
  )
}

export function WorkersStrip({
  workers,
  loading,
}: {
  workers: WorkerRow[] | undefined
  loading: boolean
}) {
  if (loading) {
    return (
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-52 rounded-md" />
        ))}
      </div>
    )
  }

  if (!workers || workers.length === 0) {
    return (
      <p className="text-sm text-muted-foreground">No workers reported yet.</p>
    )
  }

  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
      {workers.map((worker) => (
        <WorkerCard key={worker.name} worker={worker} />
      ))}
    </div>
  )
}
