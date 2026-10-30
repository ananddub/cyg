// Config history for one game: every `games.config` change lands here as an
// append-only version row. Two versions select a diff; rollback appends the
// old body as a new version and swaps the live config in one server pass.

import { useState } from "react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { Mono, PageHeading, Panel } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import type { ConsoleConfigVersion } from "@/lib/console"
import { formatTime } from "@/lib/format"

export function ConfigHistoryView({ gameId }: { gameId: string }) {
  const base = `/v2/console/games/${gameId}/config`
  const history = useConsoleQuery<ConsoleConfigVersion[]>("config-history", `${base}/history`)

  const [left, setLeft] = useState<number | null>(null)
  const [right, setRight] = useState<number | null>(null)
  const [rollback, setRollback] = useState<ConsoleConfigVersion | null>(null)

  const rows = history.data ?? []
  const byVersion = new Map(rows.map((row) => [row.version, row]))
  const diff =
    left !== null && right !== null
      ? lineDiff(
          pretty(byVersion.get(left)?.body),
          pretty(byVersion.get(right)?.body),
        )
      : null

  return (
    <div className="flex flex-col gap-4">
      <PageHeading
        title="Config History"
        description="Every change to this game's config is a version: hash, author, timestamp. A rollback appends the old body as a new version — history is never rewritten."
      />
      <Panel title="Versions">
        {history.isError ? (
          <p className="px-4 py-6 text-center text-sm text-red-400">
            {queryError(history.error)}
          </p>
        ) : rows.length === 0 ? (
          <p className="px-4 py-6 text-center text-sm text-muted-foreground">
            No config change has been recorded yet. The first update creates version 1.
          </p>
        ) : (
          <div className="divide-y divide-border/40">
            {rows.map((row) => (
              <div key={row.version} className="flex flex-wrap items-center justify-between gap-2 px-4 py-3">
                <div className="flex items-center gap-3">
                  <input
                    type="radio"
                    aria-label={`Diff from version ${row.version}`}
                    checked={left === row.version}
                    onChange={() => setLeft(row.version)}
                  />
                  <input
                    type="radio"
                    aria-label={`Diff to version ${row.version}`}
                    checked={right === row.version}
                    onChange={() => setRight(row.version)}
                  />
                  <Mono className="font-semibold">v{row.version}</Mono>
                  {row.is_active ? (
                    <Mono className="text-emerald-300">live</Mono>
                  ) : (
                    <Mono className="text-muted-foreground">superseded</Mono>
                  )}
                  <Mono className="text-muted-foreground">{row.sha256.slice(0, 12)}</Mono>
                  <Mono className="text-muted-foreground">{formatTime(row.create_time)}</Mono>
                  {row.author !== null && (
                    <Mono className="text-muted-foreground">by {row.author}</Mono>
                  )}
                </div>
                {!row.is_active && (
                  <Button size="sm" variant="outline" onClick={() => setRollback(row)}>
                    Roll back to v{row.version}
                  </Button>
                )}
              </div>
            ))}
          </div>
        )}
      </Panel>

      {diff !== null && (
        <Panel title={`Diff v${left} → v${right}`}>
          <div className="max-h-[50vh] overflow-y-auto p-4 font-mono text-xs">
            {diff.length === 0 ? (
              <p className="text-muted-foreground">The two bodies are identical.</p>
            ) : (
              diff.map((line, index) => (
                <div
                  key={index}
                  className={
                    line.startsWith("+")
                      ? "text-emerald-300"
                      : line.startsWith("-")
                        ? "text-red-400"
                        : "text-muted-foreground"
                  }
                >
                  {line}
                </div>
              ))
            )}
          </div>
        </Panel>
      )}

      <ConfirmAction
        open={rollback !== null}
        title={`Roll back to v${rollback?.version ?? ""}`}
        message="The old body becomes a new active version; history keeps every row. Running rooms pick the config up on their next settings reload."
        confirmLabel={`Roll back to v${rollback?.version ?? ""}`}
        method="POST"
        path={rollback === null ? "" : `${base}/rollback/${rollback.version}`}
        success={rollback === null ? "" : `Rolled back to v${rollback.version}`}
        onClose={() => setRollback(null)}
      />
    </div>
  )
}

function pretty(body: unknown): string {
  return JSON.stringify(body ?? {}, null, 2)
}

/** A plain line diff of the two pretty-printed bodies: enough for a config
 * review, with no diff dependency in the bundle. */
function lineDiff(from: string, to: string): string[] {
  const a = from.split("\n")
  const b = to.split("\n")
  const common = new Set(a.filter((line) => b.includes(line)))
  const out: string[] = []
  let i = 0
  let j = 0
  while (i < a.length || j < b.length) {
    if (i < a.length && common.has(a[i]) && a[i] === b[j]) {
      out.push(`  ${a[i]}`)
      i += 1
      j += 1
    } else if (i < a.length && !common.has(a[i])) {
      out.push(`- ${a[i]}`)
      i += 1
    } else if (j < b.length && !common.has(b[j])) {
      out.push(`+ ${b[j]}`)
      j += 1
    } else {
      if (i < a.length) {
        out.push(`- ${a[i]}`)
        i += 1
      }
      if (j < b.length) {
        out.push(`+ ${b[j]}`)
        j += 1
      }
    }
  }
  return out.filter((line) => line.trim() !== "" || line === "  ")
}
