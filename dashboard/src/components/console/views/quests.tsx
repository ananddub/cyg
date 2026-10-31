// Quests & Seasons: the engagement engine's operator surface. Quests are
// goal definitions (a counter and a target) with a reward and an optional
// season window; seasons bind quests to a calendar. `GET/POST
// /v2/console/quests` + `PUT/DELETE …/{key}` and the same for `/seasons`.

import { useEffect, useState } from "react"
import { Pencil, Plus, Trash2 } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import {
  DataTable,
  Mono,
  PageHeading,
  Pager,
  Panel,
  SearchField,
  type Column,
} from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleQuest, type ConsoleSeason } from "@/lib/console"
import { formatTime, num } from "@/lib/format"
import { QuestDialog, SeasonDialog } from "./quest-dialogs"

const QUESTS = CONSOLE_ENDPOINTS.quests
const SEASONS = CONSOLE_ENDPOINTS.seasons
const PAGE_SIZE = 25

type Tab = "quests" | "seasons"

export function QuestsView() {
  const [tab, setTab] = useState<Tab>("quests")
  const [offset, setOffset] = useState(0)
  const [query, setQuery] = useState("")

  const [creatingQuest, setCreatingQuest] = useState(false)
  const [editingQuest, setEditingQuest] = useState<ConsoleQuest | null>(null)
  const [deletingQuest, setDeletingQuest] = useState<ConsoleQuest | null>(null)

  const [creatingSeason, setCreatingSeason] = useState(false)
  const [editingSeason, setEditingSeason] = useState<ConsoleSeason | null>(null)
  const [deletingSeason, setDeletingSeason] = useState<ConsoleSeason | null>(null)

  const quests = useConsoleQuery<ConsoleQuest[]>("quests", QUESTS)
  const seasons = useConsoleQuery<ConsoleSeason[]>("seasons", SEASONS)

  useEffect(() => {
    setOffset(0)
  }, [query, tab])

  const needle = query.trim().toLowerCase()
  const filteredQuests = (quests.data ?? []).filter((row) =>
    needle === ""
      ? true
      : [row.quest_key, row.title, row.goal_kind, row.season ?? ""].some((v) =>
          v.toLowerCase().includes(needle),
        ),
  )
  const filteredSeasons = (seasons.data ?? []).filter((row) =>
    needle === ""
      ? true
      : [row.season_key, row.title].some((v) => v.toLowerCase().includes(needle)),
  )

  const questColumns: Column<ConsoleQuest>[] = [
    { header: "Key", cell: (row) => <Mono>{row.quest_key}</Mono> },
    { header: "Title", cell: (row) => <span className="font-medium text-foreground">{row.title}</span> },
    {
      header: "Goal",
      cell: (row) => (
        <Mono>
          {row.goal_kind} ≥ {num(row.goal_amount)}
        </Mono>
      ),
    },
    { header: "Reward", cell: (row) => <Mono className="max-w-52 truncate">{JSON.stringify(row.reward)}</Mono> },
    {
      header: "Season",
      cell: (row) =>
        row.season ? <Badge variant="secondary">{row.season}</Badge> : <Mono className="text-muted-foreground">—</Mono>,
    },
    {
      header: "Window",
      cell: (row) => <Mono>{`${formatTime(row.active_from)} → ${formatTime(row.active_to)}`}</Mono>,
    },
    {
      header: "Status",
      cell: (row) => {
        const now = Date.now()
        const live = Date.parse(row.active_from) <= now && now <= Date.parse(row.active_to)
        return live ? <Badge variant="secondary">live</Badge> : <Badge variant="outline">scheduled</Badge>
      },
    },
    {
      header: "Actions",
      cell: (row) => (
        <div className="flex gap-1.5">
          <Button variant="ghost" size="icon-xs" aria-label={`Edit ${row.quest_key}`} onClick={() => setEditingQuest(row)}>
            <Pencil />
          </Button>
          <Button variant="ghost" size="icon-xs" aria-label={`Delete ${row.quest_key}`} onClick={() => setDeletingQuest(row)}>
            <Trash2 />
          </Button>
        </div>
      ),
    },
  ]

  const seasonColumns: Column<ConsoleSeason>[] = [
    { header: "Key", cell: (row) => <Mono>{row.season_key}</Mono> },
    { header: "Title", cell: (row) => <span className="font-medium text-foreground">{row.title}</span> },
    {
      header: "Runs",
      cell: (row) => <Mono>{`${formatTime(row.starts_at)} → ${formatTime(row.ends_at)}`}</Mono>,
    },
    {
      header: "Reward track",
      cell: (row) => <Mono className="max-w-52 truncate">{JSON.stringify(row.reward_track)}</Mono>,
    },
    {
      header: "Actions",
      cell: (row) => (
        <div className="flex gap-1.5">
          <Button variant="ghost" size="icon-xs" aria-label={`Edit ${row.season_key}`} onClick={() => setEditingSeason(row)}>
            <Pencil />
          </Button>
          <Button variant="ghost" size="icon-xs" aria-label={`Delete ${row.season_key}`} onClick={() => setDeletingSeason(row)}>
            <Trash2 />
          </Button>
        </div>
      ),
    },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <PageHeading
            title="Quests"
            description="Engagement as data: a counter, a target and a reward. Players progress through gameplay events; claiming pays out once. Seasons bind quests to a calendar window."
          />
        </div>
        <Button
          className="h-9 shrink-0 gap-1.5"
          onClick={() => (tab === "quests" ? setCreatingQuest(true) : setCreatingSeason(true))}
        >
          <Plus className="size-3.5" />
          {tab === "quests" ? "New quest" : "New season"}
        </Button>
      </div>

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
        <div className="flex rounded-lg border border-border bg-muted/80 p-0.5 shadow-xs">
          {(["quests", "seasons"] as Tab[]).map((t) => (
            <button
              key={t}
              type="button"
              onClick={() => setTab(t)}
              className={`rounded px-3 py-1.5 text-xs font-semibold capitalize transition ${
                tab === t ? "bg-card text-foreground shadow-xs" : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {t}
            </button>
          ))}
        </div>
        <SearchField
          value={query}
          onChange={setQuery}
          placeholder={tab === "quests" ? "Search quests by key, title, counter or season" : "Search seasons by key or title"}
          label={tab === "quests" ? "Search quests" : "Search seasons"}
          className="sm:flex-1"
        />
      </div>

      <Panel fill>
        {tab === "quests" ? (
          <DataTable
            columns={questColumns}
            rows={filteredQuests}
            loading={quests.isPending}
            error={queryError(quests.error)}
            rowKey={(row) => row.quest_key}
            empty={
              (quests.data ?? []).length > 0
                ? "No quests match this search."
                : "No quests yet — the first one turns a counter into a habit."
            }
            fill
          />
        ) : (
          <DataTable
            columns={seasonColumns}
            rows={filteredSeasons}
            loading={seasons.isPending}
            error={queryError(seasons.error)}
            rowKey={(row) => row.season_key}
            empty={
              (seasons.data ?? []).length > 0
                ? "No seasons match this search."
                : "No seasons yet — bind quests to a calendar window."
            }
            fill
          />
        )}
        <Pager
          offset={offset}
          limit={PAGE_SIZE}
          count={(tab === "quests" ? filteredQuests : filteredSeasons).length}
          onChange={setOffset}
        />
      </Panel>

      <QuestDialog quest={creatingQuest ? null : editingQuest} open={creatingQuest || editingQuest !== null} onClose={() => { setCreatingQuest(false); setEditingQuest(null) }} seasons={seasons.data ?? []} />
      <SeasonDialog season={creatingSeason ? null : editingSeason} open={creatingSeason || editingSeason !== null} onClose={() => { setCreatingSeason(false); setEditingSeason(null) }} />

      <ConfirmAction
        open={deletingQuest !== null}
        title={`Delete ${deletingQuest?.quest_key ?? ""}`}
        message="The quest definition is removed; player progress against it stays but can never be claimed."
        confirmLabel="Delete quest"
        destructive
        method="DELETE"
        path={deletingQuest ? `${QUESTS}/${encodeURIComponent(deletingQuest.quest_key)}` : ""}
        success={`Deleted ${deletingQuest?.quest_key ?? ""}`}
        onClose={() => setDeletingQuest(null)}
      />
      <ConfirmAction
        open={deletingSeason !== null}
        title={`Delete ${deletingSeason?.season_key ?? ""}`}
        message="The season is removed; quests bound to it keep running without a window."
        confirmLabel="Delete season"
        destructive
        method="DELETE"
        path={deletingSeason ? `${SEASONS}/${encodeURIComponent(deletingSeason.season_key)}` : ""}
        success={`Deleted ${deletingSeason?.season_key ?? ""}`}
        onClose={() => setDeletingSeason(null)}
      />
    </div>
  )
}
