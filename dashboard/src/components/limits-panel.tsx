import { useEffect, useRef, useState } from "react"

import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import type { RateLimitHealth, SeatHealth } from "@/lib/types"

type Props = {
  seats: SeatHealth | undefined
  rateLimit: RateLimitHealth | undefined
  agones: Record<string, unknown> | undefined
  loading: boolean
}

function fmt(n: number | undefined | null): string {
  return typeof n === "number" ? n.toLocaleString() : "—"
}

// `true` while the counter grew between two snapshots — the "is climbing"
// the panel needs, which a single snapshot cannot express on its own.
function useClimbing(value: number | undefined): boolean {
  const [climbing, setClimbing] = useState(false)
  const prev = useRef<number | undefined>(undefined)
  useEffect(() => {
    if (typeof value !== "number") {
      setClimbing(false)
    } else if (typeof prev.current === "number" && value > prev.current) {
      setClimbing(true)
    } else {
      setClimbing(false)
    }
    prev.current = value
  }, [value])
  return climbing
}

function Stat({ label, value, alarm }: { label: string; value: string; alarm?: boolean }) {
  return (
    <div>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={`font-mono text-sm tabular-nums ${alarm ? "text-destructive" : ""}`}>{value}</div>
    </div>
  )
}

// The sidecar's snapshot is Agones' own JSON; read each key defensively so a
// shape we have never seen renders dashes instead of throwing.
function digBoolean(a: Record<string, unknown> | undefined, key: string): boolean | undefined {
  if (!a) return undefined
  if (typeof a[key] === "boolean") return a[key] as boolean
  const status = a.status
  if (status && typeof status === "object" && typeof (status as Record<string, unknown>)[key] === "boolean") {
    return (status as Record<string, unknown>)[key] as boolean
  }
  return undefined
}

function digString(a: Record<string, unknown> | undefined, key: string): string | undefined {
  if (!a) return undefined
  if (typeof a[key] === "string") return a[key] as string
  const status = a.status
  if (status && typeof status === "object" && typeof (status as Record<string, unknown>)[key] === "string") {
    return (status as Record<string, unknown>)[key] as string
  }
  return undefined
}

export function LimitsPanel({ seats, rateLimit, agones, loading }: Props) {
  const refusedClimbing = useClimbing(seats?.refused)
  const deniedClimbing = useClimbing(rateLimit?.denied)

  if (loading && seats === undefined && rateLimit === undefined) {
    return (
      <div className="grid gap-4 sm:grid-cols-2">
        <Card>
          <CardHeader><CardTitle>Seats</CardTitle></CardHeader>
          <CardContent className="grid grid-cols-4 gap-2">{[0, 1, 2, 3].map((i) => <Skeleton key={i} className="h-9" />)}</CardContent>
        </Card>
        <Card>
          <CardHeader><CardTitle>Rate limit</CardTitle></CardHeader>
          <CardContent className="grid grid-cols-5 gap-2">{[0, 1, 2, 3, 4].map((i) => <Skeleton key={i} className="h-9" />)}</CardContent>
        </Card>
      </div>
    )
  }

  const agonesEnabled = digBoolean(agones, "enabled")
  const agonesReady = digBoolean(agones, "ready") ?? digBoolean(agones, "isReady")
  const agonesState = digString(agones, "state") ?? digString(agones, "status")

  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <Card>
        <CardHeader>
          <CardTitle>Seats</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="grid grid-cols-4 gap-2">
            <Stat label="Outstanding" value={fmt(seats?.outstanding)} />
            <Stat label="Minted" value={fmt(seats?.minted)} />
            <Stat label="Rejected" value={fmt(seats?.rejected)} />
            <Stat label="Refused" value={fmt(seats?.refused)} alarm={refusedClimbing} />
          </div>
          <div className="text-xs text-muted-foreground">
            {seats ? `${fmt(seats.bound)} bound · ttl ${fmt(seats.ttlSecs)}s` : "—"}
          </div>
          {refusedClimbing && (
            <Badge variant="destructive">refusing — seats expiring unfilled</Badge>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Rate limit</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="grid grid-cols-5 gap-2">
            <Stat label="Allowed" value={fmt(rateLimit?.allowed)} />
            <Stat label="Denied" value={fmt(rateLimit?.denied)} alarm={deniedClimbing} />
            <Stat label="Squeezed" value={fmt(rateLimit?.squeezed)} />
            <Stat label="Swept" value={fmt(rateLimit?.swept)} />
            <Stat label="Tracked" value={fmt(rateLimit?.tracked)} />
          </div>
          <div className="text-xs text-muted-foreground">
            {rateLimit ? `configured ${fmt(rateLimit.rps)} rps · burst ${fmt(rateLimit.burst)}` : "—"}
          </div>
          {deniedClimbing && (
            <Badge variant="destructive">denied growing — traffic above budget</Badge>
          )}
        </CardContent>
      </Card>

      <Card size="sm" className="sm:col-span-2">
        <CardContent className="flex flex-wrap items-center gap-3">
          <span className="text-xs font-medium">Agones</span>
          <Badge variant={agonesEnabled === true ? "secondary" : "outline"}>
            {agonesEnabled === undefined ? "enabled —" : agonesEnabled ? "enabled" : "disabled"}
          </Badge>
          <span className="text-xs text-muted-foreground">
            state <span className="font-mono">{agonesState ?? "—"}</span>
          </span>
          <span className="text-xs text-muted-foreground">
            ready{" "}
            <span className="font-mono">
              {agonesReady === undefined ? "—" : agonesReady ? "yes" : "no"}
            </span>
          </span>
        </CardContent>
      </Card>
    </div>
  )
}
