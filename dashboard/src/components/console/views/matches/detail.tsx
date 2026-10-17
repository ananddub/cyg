// One match's live state: the members joined right now, its tick, and the button
// that stops it through the server's match lifecycle.
//
// The state comes from `GET /v2/console/match/{id}/state`, which for a relay match
// answers the real presences with tick 0 and an empty handler state — the same
// answer reference gives a relayed match, because there is no handler holding state.
// Terminating routes `DELETE /v2/console/match/{id}`, which runs the match_handler
// lifecycle (members are notified, the match leaves the registry) rather than a
// raw record delete.

import { useState } from "react"
import { Trash2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, Field, Mono, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import {
  CONSOLE_ENDPOINTS,
  type ConsoleMatch,
  type ConsoleMatchPresence,
  type ConsoleMatchState,
} from "@/lib/console"
import { dash, num, shortId } from "@/lib/format"

const MATCHES = CONSOLE_ENDPOINTS.matches

export function MatchDetail({ match, onDeleted }: { match: ConsoleMatch; onDeleted: () => void }) {
  const [terminating, setTerminating] = useState(false)
  const id = match.api_match.match_id

  const state = useConsoleQuery<ConsoleMatchState>("match-state", `${MATCHES}/${id}/state`)

  const columns: Column<ConsoleMatchPresence>[] = [
    {
      header: "Player",
      cell: (row) => (
        <span className="text-foreground">
          {dash(row.username)} <Mono>{shortId(row.user_id)}</Mono>
        </span>
      ),
    },
    { header: "Session", cell: (row) => <Mono>{shortId(row.session_id)}</Mono> },
    { header: "Node", cell: (row) => dash(row.node) },
  ]

  return (
    <div className="space-y-4">
      <Panel
        title={shortId(id)}
        description={dash(match.api_match.label)}
        actions={
          <Button size="sm" variant="destructive" onClick={() => setTerminating(true)}>
            <Trash2 />
            Terminate
          </Button>
        }
      >
        <div className="px-4 py-2">
          <Field label="Type">{match.api_match.authoritative ? "Authoritative" : "Relay"}</Field>
          <Field label="Players">{num(match.api_match.size)}</Field>
          <Field label="Tick rate">
            {match.api_match.tick_rate === undefined ? "—" : num(match.api_match.tick_rate)}
          </Field>
          <Field label="Handler">{dash(match.api_match.handler_name)}</Field>
          <Field label="Node">{dash(match.node)}</Field>
        </div>
      </Panel>

      <Panel
        title="Live state"
        description={state.isPending ? "Loading…" : `${state.data?.presences.length ?? 0} present`}
      >
        <div className="px-4 py-2">
          <Field label="Tick">{num(state.data?.tick ?? 0)}</Field>
          <Field label="Handler state">{state.data?.state === "" || state.data === undefined ? "—" : "reported"}</Field>
        </div>
        <DataTable
          columns={columns}
          rows={state.data?.presences}
          loading={state.isPending}
          error={queryError(state.error)}
          rowKey={(row) => row.session_id}
          empty="No players joined."
        />
      </Panel>

      <ConfirmAction
        open={terminating}
        onClose={() => setTerminating(false)}
        onDone={onDeleted}
        title="Terminate match"
        message="The match is stopped through its lifecycle handler: joined players are notified and it leaves the running set. This cannot be undone."
        confirmLabel="Terminate match"
        destructive
        method="DELETE"
        path={`${MATCHES}/${id}`}
        success="Match terminated."
      />
    </div>
  )
}
