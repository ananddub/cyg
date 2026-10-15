import { useEffect, useState } from "react"

import logoUrl from "@/assets/logo.png"
import type { Overview } from "@/lib/types"
import { cn } from "@/lib/utils"

export interface DashboardHeaderProps {
  overview?: Overview
  /** "ws" while the dashboard socket drives updates, "poll" on the REST fallback. */
  source: "ws" | "poll"
  connected: boolean
  /** Local wall-clock ms of the last WS frame; undefined before the first. */
  lastFrameAtMs?: number
  sectionLabel?: string
  roomName?: string | null
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
  live: { label: "LIVE", dot: "bg-primary animate-pulse", text: "text-primary" },
  reconnecting: { label: "RECONNECTING", dot: "bg-warning", text: "text-warning" },
  offline: { label: "OFFLINE", dot: "bg-destructive", text: "text-destructive" },
  poll: { label: "POLL", dot: "bg-muted-foreground", text: "text-muted-foreground" },
}

function formatSampleAge(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms)) return "—"
  if (ms < 1000) return `${Math.round(ms)} ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`
  return `${Math.floor(ms / 60_000)}m ${Math.floor((ms % 60_000) / 1000)}s`
}

export function DashboardHeader({
  overview,
  source,
  connected,
  lastFrameAtMs,
  sectionLabel = "Overview",
  roomName,
}: DashboardHeaderProps) {
  const [now, setNow] = useState(() => Date.now())

  // One shared per-second tick drives both the staleness cut-off and the clock,
  // so the pill and the displayed time can never disagree.
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(id)
  }, [])

  const frameAgeMs = lastFrameAtMs == null ? null : Math.max(0, now - lastFrameAtMs)
  const pill = pillStyle[resolvePill(source, connected, frameAgeMs)]
  const server = overview?.server
  const sampleAge = overview?.sampleAgeMs

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

      {/* Right side: Pill, Server info, Sample age, Time */}
      <div className="flex items-center gap-3">
        <span className="inline-flex shrink-0 items-center gap-1.5 rounded-sm border border-border px-2 py-0.5 text-xs font-medium">
          <span className={cn("size-1.5 rounded-full", pill.dot)} />
          <span className={pill.text}>{pill.label}</span>
        </span>

        {server && (
          <span className="hidden sm:inline text-xs text-muted-foreground tabular-nums shrink-0">
            {server.name || "room"} <span className="text-foreground font-mono">:{server.controlPort}</span>
          </span>
        )}

        <span
          className="hidden md:inline text-xs text-muted-foreground tabular-nums shrink-0"
          title="Age of the newest sim sample on this control plane"
        >
          sample {formatSampleAge(sampleAge)} ago
        </span>

        <time className="hidden lg:inline text-xs text-muted-foreground tabular-nums shrink-0">
          {new Date(now).toLocaleTimeString()}
        </time>
      </div>
    </header>
  )
}
