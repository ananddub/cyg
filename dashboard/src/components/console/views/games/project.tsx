// One game's settings page: the same detail the catalog's manage sheet shows,
// rendered as an area so the workspace owns configuration without a modal.
// The detail read is the only surface that carries the API key and module
// figures, so this view re-reads it by id rather than trusting the list row.

import { PageHeading, Panel } from "@/components/console/data-table"
import { queryError, useConsoleQuery } from "@/hooks/use-console"

import { GameDetailView } from "./detail"
import type { GameDetail } from "./model"
import { gamePath } from "./routes"

export function GameProjectView({
  gameId,
  onArchived,
}: {
  gameId: string
  onArchived: () => void
}) {
  const detail = useConsoleQuery<GameDetail>(`game-${gameId}`, gamePath(gameId))

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Project Settings"
        description="This game's configuration, credentials and deployed room logic. Changes here apply to this project only."
      />

      {detail.isPending && (
        <Panel title="Loading project">
          <p className="px-4 py-6 text-sm text-muted-foreground">Loading…</p>
        </Panel>
      )}

      {detail.error !== null && detail.error !== undefined && (
        <Panel title="Project unavailable">
          <p className="px-4 py-6 text-sm text-destructive">{queryError(detail.error)}</p>
        </Panel>
      )}

      {detail.data !== undefined && (
        <GameDetailView key={detail.data.id} game={detail.data} onArchived={onArchived} />
      )}
    </div>
  )
}
