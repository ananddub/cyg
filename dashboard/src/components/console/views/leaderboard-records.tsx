// The selected leaderboard: its definition, its records (`GET …/{id}/records`)
// and the per-owner record reset (`DELETE …/{id}/owner/{owner_id}`).

import { useState } from "react"
import { RotateCcw } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { Field, JsonBlock, Mono, Panel } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleLeaderboard, type ConsoleRecord } from "@/lib/console"
import { dash, formatTime, num } from "@/lib/format"

/** The record's owner label: the username when the row carries one, else its id. */
function ownerLabel(record: ConsoleRecord): string {
  return dash(record.username) === "—" ? record.owner_id : record.username
}

export function RecordsPanel({
  board,
  onRequestDelete,
}: {
  board: ConsoleLeaderboard
  onRequestDelete: () => void
}) {
  const records = useConsoleQuery<ConsoleRecord[]>(
    "board-records",
    // A board id is an operator-chosen string, so it is escaped, not assumed safe.
    `${CONSOLE_ENDPOINTS.leaderboards}/${encodeURIComponent(board.id)}/records`,
  )
  const [resetOwner, setResetOwner] = useState<ConsoleRecord | null>(null)

  return (
    <div className="space-y-4">
      <Panel title="Board" description={board.id}>
        <div className="px-4 py-2">
          <Field label="Title">{board.title}</Field>
          <Field label="Records">{num(board.size)}</Field>
          <Field label="Max score">{board.max_num_score === 0 ? "—" : num(board.max_num_score)}</Field>
          <Field label="Authoritative">{board.authoritative ? "yes" : "no"}</Field>
          <Field label="Created">{formatTime(board.create_time)}</Field>
        </div>
      </Panel>

      <Panel
        title="Records"
        description={`${records.data?.length ?? 0} listed`}
        actions={
          <Button size="sm" variant="destructive" onClick={onRequestDelete}>
            Delete board
          </Button>
        }
      >
        {queryError(records.error) !== null ? (
          <p className="px-4 py-6 text-sm text-destructive">{queryError(records.error)}</p>
        ) : records.data === undefined || records.data.length === 0 ? (
          <p className="px-4 py-6 text-sm text-muted-foreground">No records.</p>
        ) : (
          <div className="divide-y divide-border/40">
            {records.data.map((record, index) => (
              <div key={`${record.owner_id}-${index}`} className="flex items-center justify-between gap-3 px-4 py-2.5">
                <div className="min-w-0">
                  <p className="truncate text-xs font-medium text-foreground">
                    {index + 1}. {ownerLabel(record)}
                  </p>
                  <Mono>{formatTime(record.update_time)}</Mono>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  <span className="font-mono text-sm tabular-nums text-primary">{num(record.score)}</span>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label={`Reset ${ownerLabel(record)}`}
                    onClick={() => setResetOwner(record)}
                  >
                    <RotateCcw />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </Panel>

      <Panel title="Metadata">
        <div className="p-3">
          <JsonBlock value={board.metadata} />
        </div>
      </Panel>

      {/* One dialog for every row: the row travels as state, so the hook behind
          the confirm is never duplicated per record. */}
      <ConfirmAction
        open={resetOwner !== null}
        onClose={() => setResetOwner(null)}
        title="Reset owner record"
        message={`The record "${resetOwner === null ? "" : ownerLabel(resetOwner)}" holds on "${board.id}" is removed; their next write starts fresh.`}
        confirmLabel="Reset record"
        destructive
        method="DELETE"
        path={`${CONSOLE_ENDPOINTS.leaderboards}/${encodeURIComponent(board.id)}/owner/${encodeURIComponent(resetOwner?.owner_id ?? "")}`}
        success="Owner record reset."
      />
    </div>
  )
}
