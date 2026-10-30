// The live log view. It owns its own WebSocket (the metrics hook filters to
// hello/snapshot/live frames, and a page that is not open must not cost the
// server a log subscription), asks the socket to start mirroring, and sheds
// old lines so a chatty server cannot grow this tab forever.

import { useEffect, useMemo, useRef, useState } from "react"

import { Button } from "@/components/ui/button"
import { FilterPills, Mono, PageHeading, Panel } from "@/components/console/data-table"
import { wsUrl } from "@/lib/api"
import type { ConsoleLogLine } from "@/lib/console"
import { useSession } from "@/lib/session"

function clock(atMs: number): string {
    return new Date(atMs).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
    })
}

const BUFFER_MAX = 500

type LevelId = "all" | "error" | "warn" | "info" | "debug" | "trace"

const LEVELS: { id: LevelId; label: string }[] = [
  { id: "all", label: "All" },
  { id: "error", label: "Error" },
  { id: "warn", label: "Warn" },
  { id: "info", label: "Info" },
  { id: "debug", label: "Debug" },
  { id: "trace", label: "Trace" },
]

const LEVEL_CLASS: Record<string, string> = {
  ERROR: "text-red-400",
  WARN: "text-amber-300",
  INFO: "text-emerald-300",
  DEBUG: "text-sky-300",
  TRACE: "text-muted-foreground",
}

const LEVEL_RANK: Record<string, number> = {
  ERROR: 0,
  WARN: 1,
  INFO: 2,
  DEBUG: 3,
  TRACE: 4,
}

export function LogsView() {
  const token = useSession()?.token ?? ""
  const [status, setStatus] = useState<"connecting" | "open" | "closed">("connecting")
  const [lines, setLines] = useState<ConsoleLogLine[]>([])
  const [level, setLevel] = useState<LevelId>("all")
  const [needle, setNeedle] = useState("")
  const [paused, setPaused] = useState(false)
  const socketRef = useRef<WebSocket | null>(null)
  const pausedRef = useRef(paused)
  pausedRef.current = paused

  useEffect(() => {
    if (token === "") return
    const socket = new WebSocket(wsUrl(null))
    socketRef.current = socket
    socket.onopen = () => {
      setStatus("open")
      socket.send(JSON.stringify({ type: "logs.subscribe", level: "trace" }))
    }
    socket.onmessage = (event) => {
      if (pausedRef.current) return
      let frame: unknown
      try {
        frame = JSON.parse(String(event.data))
      } catch {
        return
      }
      const record = frame as { type?: string; lines?: ConsoleLogLine[] }
      if (record.type !== "logs" || !Array.isArray(record.lines)) return
      setLines((previous) => [...previous, ...record.lines!].slice(-BUFFER_MAX))
    }
    socket.onclose = () => setStatus("closed")
    return () => {
      socket.onclose = null
      socket.close()
      socketRef.current = null
    }
  }, [token])

  const visible = useMemo(() => {
    const floor = LEVEL_RANK[level.toUpperCase()] ?? 0
    const query = needle.trim().toLowerCase()
    return lines.filter((line) => {
      if ((LEVEL_RANK[line.level] ?? 99) > floor) return false
      if (query === "") return true
      return `${line.target} ${line.message}`.toLowerCase().includes(query)
    })
  }, [lines, level, needle])

  const bottom = useRef<HTMLDivElement | null>(null)
  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" })
  }, [visible.length])

  return (
    <div className="flex flex-col gap-4">
      <PageHeading
        title="Live Logs"
        description="The control plane's own tracing feed, mirrored over the dashboard socket. A client that falls behind is skipped, never blocked."
      />
      <Panel
        title={
          status === "open"
            ? "Streaming"
            : status === "connecting"
              ? "Connecting…"
              : "Disconnected — reload to reconnect"
        }
        actions={
          <div className="flex items-center gap-2">
            <input
              className="h-8 w-44 rounded-md border border-border bg-background px-2 text-xs"
              placeholder="filter target or text…"
              value={needle}
              onChange={(event) => setNeedle(event.target.value)}
            />
            <FilterPills<LevelId> filters={LEVELS} value={level} onChange={setLevel} />
            <Button size="sm" variant="outline" onClick={() => setPaused((value) => !value)}>
              {paused ? "Resume" : "Pause"}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setLines([])}>
              Clear
            </Button>
          </div>
        }
      >
        <div className="max-h-[60vh] min-h-64 overflow-y-auto px-4 py-3 font-mono text-xs">
          {visible.length === 0 ? (
            <p className="py-6 text-center text-sm text-muted-foreground">
              No lines{level === "all" ? "" : ` at ${level} or worse`} yet.
            </p>
          ) : (
            visible.map((line, index) => (
              <div key={`${line.atMs}-${index}`} className="flex gap-3 whitespace-pre-wrap">
                <Mono className="shrink-0 text-muted-foreground">
                  {clock(line.atMs)}
                </Mono>
                <Mono className={`w-12 shrink-0 ${LEVEL_CLASS[line.level] ?? ""}`}>
                  {line.level}
                </Mono>
                <Mono className="shrink-0 text-muted-foreground">{line.target}</Mono>
                <span className="text-foreground">{line.message}</span>
              </div>
            ))
          )}
          <div ref={bottom} />
        </div>
      </Panel>
    </div>
  )
}
