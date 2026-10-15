import { useEffect, useRef } from "react"
import { toast } from "sonner"

import type { MetricsWs } from "@/hooks/use-metrics-ws"
import type { Overview, RoomRow, RoomStatus } from "@/lib/types"

// Every toast fires on a state transition only: a page fed once per second must
// not re-announce the same outage, empty room or drop ramp every frame.
export function useDashboardToasts(
  ws: MetricsWs,
  overview: Overview | null,
  rooms: RoomRow[] | null,
) {
  const wsWasOpen = useRef(false)
  const wsSeen = useRef(false)
  const roomStatuses = useRef(new Map<string, RoomStatus>())
  const dropState = useRef({ total: 0, rising: false })

  useEffect(() => {
    const open = ws.status === "open"
    if (open && !wsWasOpen.current) {
      // The first open on page load is not a recovery worth a toast.
      if (wsSeen.current) toast.success("Live socket restored")
      wsSeen.current = true
    } else if (!open && wsWasOpen.current) {
      toast.warning("Dashboard socket dropped — falling back to polling")
    }
    wsWasOpen.current = open
  }, [ws.status])

  useEffect(() => {
    if (rooms === null) return
    for (const room of rooms) {
      const key = `${room.serverId}/${room.name}`
      const before = roomStatuses.current.get(key)
      if (before === "live" && room.status === "empty") {
        toast.info(`Room ${room.name} is now empty`)
      }
      roomStatuses.current.set(key, room.status)
    }
  }, [rooms])

  useEffect(() => {
    const total = overview?.bus.deltasDroppedFull ?? 0
    const prev = dropState.current
    // A counter that went positive long ago is "not rising"; only the
    // flat → climbing edge earns a toast.
    const rising = total > prev.total
    if (rising && !prev.rising) {
      toast.warning(`Deltas dropping under backpressure (${total.toLocaleString()} so far)`)
    }
    dropState.current = { total: Math.max(total, prev.total), rising }
  }, [overview])
}
