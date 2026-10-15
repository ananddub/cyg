import { useEffect, useState } from "react"

function formatAge(ms: number): string {
  const s = Math.floor(ms / 1000)
  if (s < 60) return `${s}s ago`
  return `${Math.floor(s / 60)}m ${s % 60}s ago`
}

/**
 * Neither the socket nor the REST poll has produced an overview; say so loudly
 * and date the last frame the page actually saw (`null` = never).
 */
export function OfflineBanner({ lastGoodAtMs }: { lastGoodAtMs: number | null }) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(id)
  }, [])
  return (
    <div
      role="status"
      className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive"
    >
      Control plane unreachable — neither the socket nor polling returns data.
      {lastGoodAtMs === null
        ? " No frame received since page load."
        : ` Last successful frame ${formatAge(now - lastGoodAtMs)}.`}
    </div>
  )
}
