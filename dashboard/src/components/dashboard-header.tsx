import { useEffect, useState } from "react"

import logoUrl from "@/assets/logo.png"
import { cn } from "@/lib/utils"

export interface DashboardHeaderProps {
  /** "ws" while the dashboard socket drives updates, "poll" on the REST fallback. */
  source: "ws" | "poll"
  connected: boolean
  /** Local wall-clock ms of the last WS frame; undefined before the first. */
  lastFrameAtMs?: number
  sectionLabel?: string
  roomName?: string | null
  /** The selected game, shown between "Console" and the area label. */
  gameName?: string | null
  /** `false` at the landing scope, where there is no plane feed to report. */
  showFeed?: boolean
}

type PillState = "live" | "reconnecting" | "offline" | "poll"

// A snapshot lands once per second, so three silent seconds already means the
// socket is open but nothing is moving behind it.
const FRAME_FRESH_MS = 3000

function resolvePill(source: string, connected: boolean, ageMs: number | null): PillState {
  if (source === "poll") return "poll"
  if (!connected) return "offline"
  if (ageMs != null && ageMs <= FRAME_FRESH_MS) return "live"
  return "reconnecting"
}

const pillStyle: Record<PillState, { label: string; dot: string; text: string }> = {
  live: { label: "LIVE", dot: "bg-emerald-500", text: "text-emerald-600 dark:text-emerald-400" },
  reconnecting: { label: "RECONNECTING", dot: "bg-warning", text: "text-warning" },
  offline: { label: "OFFLINE", dot: "bg-destructive", text: "text-destructive" },
  poll: { label: "POLL", dot: "bg-muted-foreground", text: "text-muted-foreground" },
}

export function DashboardHeader({
  source,
  connected,
  lastFrameAtMs,
  sectionLabel = "Overview",
  roomName,
  gameName,
  showFeed = true,
}: DashboardHeaderProps) {
  const [now, setNow] = useState(() => Date.now())

  // One shared per-second tick keeps the freshness cut-off honest.
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(id)
  }, [])

  const frameAgeMs = lastFrameAtMs == null ? null : Math.max(0, now - lastFrameAtMs)
  const pill = pillStyle[resolvePill(source, connected, frameAgeMs)]

  return (
    <header className="flex w-full items-center justify-between gap-x-4">
      {/* Left side: Mobile logo + Breadcrumbs (matching test.png) */}
      <div className="flex items-center gap-3">
        <img
          src={logoUrl}
          alt="cookyourgame (cfg)"
          className="h-8 w-auto object-contain shrink-0 drop-shadow-xs md:hidden"
        />

        {/* Breadcrumb matching test.png */}
        <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
          <span className="text-muted-foreground/60">Console</span>
          <span className="text-muted-foreground/40">/</span>
          {gameName !== undefined && gameName !== null && (
            <>
              <span className="max-w-40 truncate text-foreground font-medium">{gameName}</span>
              <span className="text-muted-foreground/40">/</span>
            </>
          )}
          <span className="text-foreground font-medium">{sectionLabel}</span>
          {roomName && (
            <>
              <span className="text-muted-foreground/40">/</span>
              <span className="rounded bg-muted/80 px-2 py-0.5 font-mono text-[11px] font-semibold text-foreground ring-1 ring-border/60">
                {roomName}
              </span>
            </>
          )}
        </div>
      </div>

      {/* Right side: feed status only */}
      <div className="flex items-center gap-3">
        {showFeed && (
          <span className="inline-flex shrink-0 items-center gap-1.5 rounded-sm border border-border px-2 py-0.5 text-xs font-medium">
            <span className="relative flex size-1.5 shrink-0 items-center justify-center">
              {pill.label === "LIVE" && (
                // A ping ring, not an opacity throb: it reads as "a frame just
                // landed" without making the label itself flicker.
                <span
                  className={cn(
                    "absolute inline-flex size-full rounded-full opacity-50 motion-safe:animate-ping motion-safe:[animation-duration:1.8s]",
                    pill.dot,
                  )}
                />
              )}
              <span className={cn("relative inline-flex size-1.5 rounded-full", pill.dot)} />
            </span>
            <span className={pill.text}>{pill.label}</span>
          </span>
        )}
      </div>
    </header>
  )
}
