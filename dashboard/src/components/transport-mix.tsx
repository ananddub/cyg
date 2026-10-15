import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import type { Capacity, TransportMix } from "@/lib/types"

type Props = {
  transport: TransportMix | undefined
  capacity: Capacity | undefined
  loading: boolean
}

const SEGMENTS = [
  { key: "websocket", label: "websocket", bar: "bg-chart-1" },
  { key: "webrtc", label: "webrtc", bar: "bg-chart-2" },
  { key: "native-udp", label: "native udp", bar: "bg-chart-3" },
] as const

function countOf(t: TransportMix, key: (typeof SEGMENTS)[number]["key"]): number {
  switch (key) {
    case "websocket":
      return t.websocket
    case "webrtc":
      return t.webrtc
    default:
      return t.nativeUdp
  }
}

function fmt(n: number | undefined | null): string {
  return typeof n === "number" ? String(n) : "—"
}

export function TransportMixPanel({ transport, capacity, loading }: Props) {
  if (loading && transport === undefined) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Transport mix</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <Skeleton className="h-3 w-full" />
          <Skeleton className="h-5 w-2/3" />
        </CardContent>
      </Card>
    )
  }

  const total = transport ? transport.websocket + transport.webrtc + transport.nativeUdp : 0
  // Scale to capacity when it is known so a full bar means a full room, not
  // just "more than last time".
  const cap = capacity && capacity.max > 0 ? capacity.max : 0
  const scale = Math.max(cap, total)

  return (
    <Card>
      <CardHeader>
        <CardTitle>Transport mix</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex h-3 w-full overflow-hidden rounded-xs bg-muted" role="img" aria-label={`Sessions: ${transport ? fmt(total) : "—"} of ${cap > 0 ? fmt(cap) : "unknown"}`}>
          {transport &&
            scale > 0 &&
            SEGMENTS.map((s) => {
              const n = countOf(transport, s.key)
              if (n === 0) return null
              return (
                <div
                  key={s.key}
                  className={s.bar}
                  style={{ width: `${(n / scale) * 100}%` }}
                  title={`${s.label}: ${n}`}
                />
              )
            })}
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {SEGMENTS.map((s) => (
            <Badge key={s.key} variant="outline" className="gap-1.5">
              <span className={`size-2 rounded-full ${s.bar}`} aria-hidden />
              {s.label}
              <span className="font-mono tabular-nums">{transport ? fmt(countOf(transport, s.key)) : "—"}</span>
            </Badge>
          ))}
        </div>

        <div className="grid grid-cols-3 gap-2 text-xs">
          <div>
            <div className="text-muted-foreground">Signal peers</div>
            <div className="font-mono tabular-nums text-sm">{fmt(transport?.signalPeers)}</div>
          </div>
          <div>
            <div className="text-muted-foreground">Delivered</div>
            <div className="font-mono tabular-nums text-sm">{fmt(transport?.signalsDelivered)}</div>
          </div>
          <div>
            <div className="text-muted-foreground">Dropped</div>
            <div className={`font-mono tabular-nums text-sm ${transport && transport.signalsDropped > 0 ? "text-destructive" : ""}`}>
              {fmt(transport?.signalsDropped)}
            </div>
          </div>
        </div>

        {capacity && (
          <div className="text-xs text-muted-foreground">
            {fmt(capacity.players)} of {fmt(capacity.max)} slots
          </div>
        )}
      </CardContent>
    </Card>
  )
}
