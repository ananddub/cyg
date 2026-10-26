// Server settings: the key/value document store the server reads at run time.
// A write names the whole value rather than patching it, so an edit can never
// leave a half-updated document behind.

import { useState } from "react"
import { Loader2, Pencil, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, JsonBlock, Mono, PageHeading, Panel, SearchField } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { JsonTextArea, TextInput } from "@/components/console/form"
import { JsonInline } from "@/components/console/json-view"
import { jsonText, parseJson } from "@/components/console/json"
import { DetailSheet } from "@/components/console/detail-sheet"
import { FormSheet } from "@/components/console/form-sheet"
import { useConsoleMutation, useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleSetting } from "@/lib/console"
import { formatTime } from "@/lib/format"

const SETTINGS = CONSOLE_ENDPOINTS.settings

export function SettingsView() {
  const list = useConsoleQuery<ConsoleSetting[]>("settings", SETTINGS)
  const [selectedName, setSelectedName] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)
  const [query, setQuery] = useState("")

  const selected = list.data?.find((setting) => setting.name === selectedName) ?? null

  const needle = query.trim().toLowerCase()
  const filtered = (list.data ?? []).filter((setting) => {
    if (needle === "") return true
    return [setting.name, JSON.stringify(setting.value) ?? ""].some((value) =>
      value.toLowerCase().includes(needle),
    )
  })

  const empty =
    list.data !== undefined && list.data.length > 0
      ? "No settings match this search."
      : "No settings recorded."

  const columns: Column<ConsoleSetting>[] = [
    { header: "Name", cell: (row) => <span className="font-mono text-xs font-medium text-foreground">{row.name}</span> },
    { header: "Value", cell: (row) => <JsonInline value={row.value} /> },
    { header: "Updated", cell: (row) => <Mono>{formatTime(row.update_time)}</Mono> },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <PageHeading
            title="Settings"
            description="Server-side key/value settings. A write replaces the whole value."
          />
        </div>
        <Button className="h-9 shrink-0 gap-1.5" onClick={() => setCreating(true)}>
          <Plus className="size-3.5" />
          Set a setting
        </Button>
      </div>

      <SearchField
        value={query}
        onChange={setQuery}
        placeholder="Search settings by name or value"
        label="Search settings"
      />

      <Panel title="Keys" fill>
        <DataTable
          columns={columns}
          rows={filtered}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.name}
          onRowClick={(row) => setSelectedName(row.name)}
          selectedKey={selectedName ?? undefined}
          empty={empty}
          fill
          selectable
        />
      </Panel>

      <DetailSheet
        open={selected !== null}
        onClose={() => setSelectedName(null)}
        title={selected?.name ?? "Setting"}
        description={selected ? `Updated ${formatTime(selected.update_time)}` : undefined}
      >
        {selected !== null && (
          <SettingDetail key={selected.name} setting={selected} onDeleted={() => setSelectedName(null)} />
        )}
      </DetailSheet>

      <SettingDialog open={creating} onClose={() => setCreating(false)} setting={null} />
    </div>
  )
}

function SettingDetail({ setting, onDeleted }: { setting: ConsoleSetting; onDeleted: () => void }) {
  const [editing, setEditing] = useState(false)
  const [deleting, setDeleting] = useState(false)

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <span className="text-xs font-medium text-muted-foreground">Value</span>
        <div className="flex shrink-0 gap-1.5">
          <Button size="sm" variant="outline" onClick={() => setEditing(true)}>
            <Pencil />
            Edit
          </Button>
          <Button size="sm" variant="destructive" onClick={() => setDeleting(true)}>
            <Trash2 />
            Delete
          </Button>
        </div>
      </div>

      <JsonBlock value={setting.value} />

      <p className="text-[11px] text-muted-foreground">
        A save replaces the whole value, not just part of it.
      </p>

      <SettingDialog open={editing} onClose={() => setEditing(false)} setting={setting} />
      <ConfirmAction
        open={deleting}
        onClose={() => setDeleting(false)}
        onDone={onDeleted}
        title="Delete setting"
        message="Whatever reads this key falls back to its default."
        confirmLabel="Delete"
        destructive
        method="DELETE"
        path={`${SETTINGS}/${encodeURIComponent(setting.name)}`}
        success="Setting deleted."
      />
    </div>
  )
}

/** One dialog for both acts: `setting` is the row being replaced, or `null` for a
 * new one — where the name is the operator's to type and the value is the rest. */
function SettingDialog({
  open,
  onClose,
  setting,
}: {
  open: boolean
  onClose: () => void
  setting: ConsoleSetting | null
}) {
  const [name, setName] = useState(setting?.name ?? "")
  const [value, setValue] = useState(jsonText(setting?.value))
  const write = useConsoleMutation()
  const replacing = setting !== null
  const named = replacing || name.trim() !== ""

  function close() {
    setName(setting?.name ?? "")
    setValue(jsonText(setting?.value))
    onClose()
  }

  function confirm() {
    const parsed = parseJson(value)
    if ("error" in parsed) {
      toast.error(`The value must be JSON: ${parsed.error}`)
      return
    }
    write.mutate(
      {
        method: "PUT",
        path: SETTINGS,
        // An empty field is the JSON `null`, which is a value a setting may hold.
        body: { name, value: parsed.value === undefined ? null : parsed.value },
        success: "Setting saved.",
      },
      { onSuccess: close },
    )
  }

  return (
    <FormSheet
      open={open}
      title={replacing ? `Edit ${setting.name}` : "Set a setting"}
      description="The name is the key; the value is the whole document."
      onClose={close}
      footer={
        <Button onClick={confirm} disabled={write.isPending || !named}>
          {write.isPending && <Loader2 className="animate-spin" />}
          Save
        </Button>
      }
    >
      <TextInput
        label="Name"
        hint={replacing ? "The key cannot change: renaming would leave the old row behind." : undefined}
        value={name}
        onChange={setName}
        placeholder="utc_toggle"
        mono
        autoFocus={!replacing}
        disabled={replacing}
      />
      <JsonTextArea label="Value" rows={6} value={value} onChange={setValue} placeholder="true" />
      <p className="text-[11px] text-muted-foreground">
        The value is JSON: a string is quoted, a number is not. An empty field stores null.
      </p>
    </FormSheet>
  )
}
