// Create/edit sheets for quest definitions and seasons. A quest key is
// operator-chosen and permanent; reward is a small JSON document the payout
// engine reads (e.g. {"coins": 500} or {"badge": "centurion"}).

import { useState } from "react"
import { Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { JsonTextArea, Select, TextInput } from "@/components/console/form"
import { FormSheet } from "@/components/console/form-sheet"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleQuest, type ConsoleSeason } from "@/lib/console"

const QUESTS = CONSOLE_ENDPOINTS.quests
const SEASONS = CONSOLE_ENDPOINTS.seasons

const GOAL_KIND_OPTIONS = [
  { value: "counter", label: "counter — events raise a named counter" },
  { value: "stat", label: "stat — a player stat field directly" },
]

type QuestForm = {
  quest_key: string
  title: string
  goal_kind: string
  goal_amount: string
  reward: string
  season: string
  requires_product: string
  active_from: string
  active_to: string
}

/** Stored instants are UTC while the field is read in the browser's zone, so a
 * seed→save round-trip keeps the same instant instead of shifting it. */
function toLocalInput(iso: string): string {
  const at = new Date(iso)
  if (Number.isNaN(at.getTime())) return ""
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}T${pad(at.getHours())}:${pad(at.getMinutes())}:${pad(at.getSeconds())}`
}

function seedQuest(quest: ConsoleQuest | null): QuestForm {
  return {
    quest_key: quest?.quest_key ?? "",
    title: quest?.title ?? "",
    goal_kind: quest?.goal_kind ?? "counter",
    goal_amount: quest ? String(quest.goal_amount) : "100",
    reward: quest ? JSON.stringify(quest.reward) : '{"coins": 500}',
    season: quest?.season ?? "",
    requires_product: quest?.requires_product ?? "",
    active_from: quest ? toLocalInput(quest.active_from) : "",
    active_to: quest ? toLocalInput(quest.active_to) : "",
  }
}

export function QuestDialog({
  quest,
  open,
  onClose,
  seasons,
}: {
  quest: ConsoleQuest | null
  open: boolean
  onClose: () => void
  seasons: ConsoleSeason[]
}) {
  const [form, setForm] = useState<QuestForm>(() => seedQuest(quest))
  const [rewardError, setRewardError] = useState<string | null>(null)
  const write = useConsoleMutation()
  const creating = quest === null

  function set<K extends keyof QuestForm>(key: K, value: QuestForm[K]) {
    setForm((current) => ({ ...current, [key]: value }))
  }

  const seasonOptions = [
    { value: "", label: "— no season (always-on window) —" },
    ...seasons.map((s) => ({ value: s.season_key, label: s.title })),
  ]

  function confirm() {
    let reward: unknown
    try {
      reward = JSON.parse(form.reward)
      setRewardError(null)
    } catch {
      setRewardError("Reward must be valid JSON, e.g. {\"coins\": 500}")
      return
    }
    const amount = Number(form.goal_amount)
    if (!Number.isFinite(amount) || amount <= 0) return

    const body: Record<string, unknown> = {
      quest_key: form.quest_key.trim(),
      title: form.title.trim(),
      goal_kind: form.goal_kind,
      goal_amount: amount,
      reward,
      season: form.season === "" ? null : form.season,
      requires_product: form.requires_product.trim() === "" ? null : form.requires_product.trim(),
    }
    if (form.active_from !== "") body.active_from = new Date(form.active_from).toISOString()
    if (form.active_to !== "") body.active_to = new Date(form.active_to).toISOString()

    write.mutate(
      creating
        ? { method: "POST", path: QUESTS, body, success: `Quest ${form.quest_key.trim()} created.` }
        : {
            method: "PUT",
            path: `${QUESTS}/${encodeURIComponent(quest.quest_key)}`,
            body,
            success: `Quest ${quest.quest_key} saved.`,
          },
      { onSuccess: onClose },
    )
  }

  const invalid = form.title.trim() === "" || (creating && form.quest_key.trim() === "")

  return (
    <FormSheet
      open={open}
      title={creating ? "New quest" : `Edit ${quest?.quest_key ?? ""}`}
      description="A counter, a target and a reward. Players raise the counter through gameplay events; when it passes the target, the quest can be claimed."
      onClose={onClose}
      footer={
        <Button onClick={confirm} disabled={write.isPending || invalid}>
          {write.isPending && <Loader2 className="animate-spin" />}
          {creating ? "Create quest" : "Save quest"}
        </Button>
      }
    >
      <TextInput
        label="Quest key"
        hint={creating ? "Operator-chosen and permanent — the quest's id everywhere." : "The key cannot change."}
        value={form.quest_key}
        onChange={(value) => set("quest_key", value)}
        mono
        autoFocus={creating}
        disabled={!creating}
        placeholder="e.g. centurion, daily_grinder"
      />
      <TextInput
        label="Title"
        value={form.title}
        onChange={(value) => set("title", value)}
        placeholder="e.g. Centurion — win 100 fights"
      />
      <div className="grid grid-cols-2 gap-3">
        <Select label="Goal kind" value={form.goal_kind} onChange={(value) => set("goal_kind", value)} options={GOAL_KIND_OPTIONS} />
        <TextInput
          label="Goal amount"
          hint="Counter must reach this number."
          value={form.goal_amount}
          onChange={(value) => set("goal_amount", value)}
          mono
        />
      </div>
      <div>
        <JsonTextArea
          label="Reward"
          hint='What claiming pays out, e.g. {"coins": 500} or {"badge": "centurion"}.'
          value={form.reward}
          onChange={(value) => set("reward", value)}
        />
        {rewardError && <p className="mt-1 text-[11px] font-medium text-rose-500">{rewardError}</p>}
      </div>
      <Select label="Season" value={form.season} onChange={(value) => set("season", value)} options={seasonOptions} />
      <TextInput
        label="Requires product (optional)"
        hint="Only owners of this product id see the quest."
        value={form.requires_product}
        onChange={(value) => set("requires_product", value)}
        mono
      />
      <div className="grid grid-cols-2 gap-3">
        <TextInput
          label="Active from (optional)"
          hint="ISO date-time; blank = start of time."
          value={form.active_from}
          onChange={(value) => set("active_from", value)}
          mono
          placeholder="2026-01-01T00:00"
        />
        <TextInput
          label="Active to (optional)"
          hint="ISO date-time; blank = forever."
          value={form.active_to}
          onChange={(value) => set("active_to", value)}
          mono
          placeholder="2026-12-31T23:59"
        />
      </div>
    </FormSheet>
  )
}

type SeasonForm = {
  season_key: string
  title: string
  starts_at: string
  ends_at: string
  reward_track: string
}

function seedSeason(season: ConsoleSeason | null): SeasonForm {
  return {
    season_key: season?.season_key ?? "",
    title: season?.title ?? "",
    starts_at: season ? toLocalInput(season.starts_at) : "",
    ends_at: season ? toLocalInput(season.ends_at) : "",
    reward_track: season ? JSON.stringify(season.reward_track) : "[]",
  }
}

export function SeasonDialog({
  season,
  open,
  onClose,
}: {
  season: ConsoleSeason | null
  open: boolean
  onClose: () => void
}) {
  const [form, setForm] = useState<SeasonForm>(() => seedSeason(season))
  const [trackError, setTrackError] = useState<string | null>(null)
  const write = useConsoleMutation()
  const creating = season === null

  function set<K extends keyof SeasonForm>(key: K, value: SeasonForm[K]) {
    setForm((current) => ({ ...current, [key]: value }))
  }

  function confirm() {
    let rewardTrack: unknown
    try {
      rewardTrack = JSON.parse(form.reward_track)
      setTrackError(null)
    } catch {
      setTrackError("Reward track must be valid JSON (an array works).")
      return
    }
    if (form.starts_at === "" || form.ends_at === "") return

    const body = {
      season_key: form.season_key.trim(),
      title: form.title.trim(),
      starts_at: new Date(form.starts_at).toISOString(),
      ends_at: new Date(form.ends_at).toISOString(),
      reward_track: rewardTrack,
    }

    write.mutate(
      creating
        ? { method: "POST", path: SEASONS, body, success: `Season ${form.season_key.trim()} created.` }
        : {
            method: "PUT",
            path: `${SEASONS}/${encodeURIComponent(season.season_key)}`,
            body,
            success: `Season ${season.season_key} saved.`,
          },
      { onSuccess: onClose },
    )
  }

  const invalid = form.title.trim() === "" || form.starts_at === "" || form.ends_at === "" || (creating && form.season_key.trim() === "")

  return (
    <FormSheet
      open={open}
      title={creating ? "New season" : `Edit ${season?.season_key ?? ""}`}
      description="A calendar window quests can be bound to. Inside the window its quests run; outside it they sleep."
      onClose={onClose}
      footer={
        <Button onClick={confirm} disabled={write.isPending || invalid}>
          {write.isPending && <Loader2 className="animate-spin" />}
          {creating ? "Create season" : "Save season"}
        </Button>
      }
    >
      <TextInput
        label="Season key"
        hint={creating ? "Operator-chosen and permanent." : "The key cannot change."}
        value={form.season_key}
        onChange={(value) => set("season_key", value)}
        mono
        autoFocus={creating}
        disabled={!creating}
        placeholder="e.g. winter_2026"
      />
      <TextInput
        label="Title"
        value={form.title}
        onChange={(value) => set("title", value)}
        placeholder="e.g. Winter Clash 2026"
      />
      <div className="grid grid-cols-2 gap-3">
        <TextInput
          label="Starts at"
          hint="ISO date-time."
          value={form.starts_at}
          onChange={(value) => set("starts_at", value)}
          mono
          placeholder="2026-01-01T00:00"
        />
        <TextInput
          label="Ends at"
          hint="ISO date-time."
          value={form.ends_at}
          onChange={(value) => set("ends_at", value)}
          mono
          placeholder="2026-01-31T23:59"
        />
      </div>
      <div>
        <JsonTextArea
          label="Reward track"
          hint="Ordered season rewards as JSON (array)."
          value={form.reward_track}
          onChange={(value) => set("reward_track", value)}
        />
        {trackError && <p className="mt-1 text-[11px] font-medium text-rose-500">{trackError}</p>}
      </div>
    </FormSheet>
  )
}
