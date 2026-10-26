// The tournament write dialogs. One dialog for both acts, exactly like the
// operator dialogs: `board` is the row being edited, or `null` for a create.
// The body is the console's `TournamentWriteRequest` — a field left out takes
// the column default on a create and keeps the stored value on an update.

import { useState } from "react"
import { Loader2 } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Checkbox, JsonTextArea, Select, TextInput } from "@/components/console/form"
import { jsonText, parseJson } from "@/components/console/json"
import { FormSheet } from "@/components/console/form-sheet"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleLeaderboard } from "@/lib/console"

const TOURNAMENTS = CONSOLE_ENDPOINTS.tournaments

const OPERATOR_OPTIONS = [
  { value: "0", label: "0 · none" },
  { value: "1", label: "1 · best" },
  { value: "2", label: "2 · set" },
  { value: "3", label: "3 · incremental" },
]

const SORT_OPTIONS = [
  { value: "0", label: "Ascending" },
  { value: "1", label: "Descending" },
]

interface TournamentForm {
  id: string
  title: string
  description: string
  operator: string
  sort_order: string
  duration: string
  start_time: string
  end_time: string
  reset_schedule: string
  max_size: string
  max_num_score: string
  metadata: string
  authoritative: boolean
  join_required: boolean
}

function seed(board: ConsoleLeaderboard | null): TournamentForm {
  return {
    id: board?.id ?? "",
    title: board?.title ?? "",
    description: board?.description ?? "",
    operator: String(board?.operator ?? 1),
    sort_order: String(board?.sort_order ?? 1),
    duration: board === null ? "" : String(board.duration),
    start_time: board?.start_time ?? "",
    end_time: board?.end_time ?? "",
    reset_schedule: board?.reset_schedule ?? "",
    // The row shape reports no `max_size`, so an edit leaves it at the stored value.
    max_size: "",
    max_num_score: board === null ? "" : String(board.max_num_score),
    metadata: jsonText(board?.metadata),
    authoritative: board?.authoritative ?? false,
    join_required: false,
  }
}

/** The request body: filled fields only, snake_case as the DTO spells them. */
function buildBody(form: TournamentForm, creating: boolean): Record<string, unknown> | null {
  const parsed = parseJson(form.metadata)
  if ("error" in parsed) {
    toast.error(`The metadata must be JSON: ${parsed.error}`)
    return null
  }
  const body: Record<string, unknown> = {
    title: form.title,
    description: form.description,
    operator: Number(form.operator),
    sort_order: Number(form.sort_order),
    authoritative: form.authoritative,
    join_required: form.join_required,
    reset_schedule: form.reset_schedule,
  }
  if (creating) body.id = form.id
  if (form.duration !== "") body.duration = Number(form.duration)
  if (form.start_time !== "") body.start_time = form.start_time
  if (form.end_time !== "") body.end_time = form.end_time
  if (form.max_size !== "") body.max_size = Number(form.max_size)
  if (form.max_num_score !== "") body.max_num_score = Number(form.max_num_score)
  // An empty metadata field is "not sent": a create takes `{}`, an update keeps
  // what is stored.
  if (parsed.value !== undefined) body.metadata = parsed.value
  return body
}

function MonoInput(props: { label: string; value: string; onChange: (value: string) => void; hint?: string }) {
  return <TextInput {...props} mono />
}

export function TournamentDialog({
  board,
  open,
  onClose,
}: {
  board: ConsoleLeaderboard | null
  open: boolean
  onClose: () => void
}) {
  const [form, setForm] = useState<TournamentForm>(() => seed(board))
  const write = useConsoleMutation()
  const creating = board === null

  function set<K extends keyof TournamentForm>(key: K, value: TournamentForm[K]) {
    setForm((current) => ({ ...current, [key]: value }))
  }

  function confirm() {
    const body = buildBody(form, creating)
    if (body === null) return
    write.mutate(
      creating
        ? { method: "POST", path: TOURNAMENTS, body, success: "Tournament created." }
        : {
            method: "PUT",
            path: `${TOURNAMENTS}/${encodeURIComponent(board.id)}`,
            body,
            success: "Tournament saved.",
          },
      { onSuccess: onClose },
    )
  }

  return (
    <FormSheet
      open={open}
      title={creating ? "New tournament" : `Edit ${board?.id ?? ""}`}
      description="An id and a round length are what make it a tournament; the rest keep their defaults when left blank."
      onClose={onClose}
      footer={
        <Button onClick={confirm} disabled={write.isPending || (creating && (form.id === "" || form.duration === ""))}>
          {write.isPending && <Loader2 className="animate-spin" />}
          {creating ? "Create" : "Save"}
        </Button>
      }
    >
      <TextInput
        label="ID"
        hint={creating ? "Operator-chosen and permanent — the board's route key." : "The id cannot change."}
        value={form.id}
        onChange={(value) => set("id", value)}
        mono
        autoFocus={creating}
        disabled={!creating}
      />
      <TextInput label="Title" value={form.title} onChange={(value) => set("title", value)} />
      <TextInput label="Description" value={form.description} onChange={(value) => set("description", value)} />
      <div className="grid grid-cols-2 gap-3">
        <Select label="Operator" value={form.operator} onChange={(value) => set("operator", value)} options={OPERATOR_OPTIONS} />
        <Select label="Sort order" value={form.sort_order} onChange={(value) => set("sort_order", value)} options={SORT_OPTIONS} />
        <MonoInput label="Duration (secs)" hint="A tournament round needs at least one second." value={form.duration} onChange={(value) => set("duration", value)} />
        <MonoInput label="Reset schedule" hint="A cron expression; blank clears the round clock." value={form.reset_schedule} onChange={(value) => set("reset_schedule", value)} />
        <MonoInput label="Start time" hint="RFC 3339, e.g. 2026-01-01T00:00:00Z" value={form.start_time} onChange={(value) => set("start_time", value)} />
        <MonoInput label="End time" hint="RFC 3339; blank keeps the stored window." value={form.end_time} onChange={(value) => set("end_time", value)} />
        <MonoInput label="Max size" value={form.max_size} onChange={(value) => set("max_size", value)} />
        <MonoInput label="Max num score" value={form.max_num_score} onChange={(value) => set("max_num_score", value)} />
      </div>
      <Checkbox label="Authoritative" checked={form.authoritative} onChange={(value) => set("authoritative", value)} />
      <Checkbox label="Join required" checked={form.join_required} onChange={(value) => set("join_required", value)} />
      <JsonTextArea label="Metadata" rows={4} value={form.metadata} onChange={(value) => set("metadata", value)} />
    </FormSheet>
  )
}
