// Server settings: the key/value document store the server reads at run time.
// A write names the whole value rather than patching it, so an edit can never
// leave a half-updated document behind.

import { useState } from "react"
import { Loader2, Pencil, Plus, Trash2 } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, JsonBlock, Mono, PageHeading, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { TextArea, TextInput } from "@/components/console/form"
import { jsonText, parseJson } from "@/components/console/json"
import { Modal } from "@/components/console/modal"
import { useConsoleMutation, useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleSetting } from "@/lib/console"
import { formatTime } from "@/lib/format"

const SETTINGS = CONSOLE_ENDPOINTS.settings

/** One line of a value, short enough for a table cell. */
function preview(value: unknown): string {
  const text = JSON.stringify(value)
  if (text === undefined) return "—"
  return text.length > 60 ? `${text.slice(0, 60)}…` : text
}

export function SettingsView() {
  const list = useConsoleQuery<ConsoleSetting[]>("settings", SETTINGS)
  const [selectedName, setSelectedName] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)

  const selected = list.data?.find((setting) => setting.name === selectedName) ?? null

  const columns: Column<ConsoleSetting>[] = [
    { header: "Name", cell: (row) => <span className="font-mono text-xs font-medium text-foreground">{row.name}</span> },
    { header: "Value", cell: (row) => <span className="text-muted-foreground">{preview(row.value)}</span> },
    { header: "Updated", cell: (row) => <Mono>{formatTime(row.update_time)}</Mono> },
  ]

  return (
    <div className="space-y-6">
      <PageHeading
        title="Settings"
        description="Server-side key/value settings. A write replaces the whole value."
      />

      <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_24rem]">
        <Panel
          title="Settings"
          actions={
            <Button size="sm" variant="outline" onClick={() => setCreating(true)}>
              <Plus />
              Set a setting
            </Button>
          }
        >
          <DataTable
            columns={columns}
            rows={list.data}
            loading={list.isPending}
            error={queryError(list.error)}
            rowKey={(row) => row.name}
            onRowClick={(row) => setSelectedName(row.name)}
            selectedKey={selectedName ?? undefined}
            empty="No settings recorded."
          />
        </Panel>

        {selected === null ? (
          <Panel title="Setting" description="No setting selected">
            <p className="px-4 py-6 text-sm text-muted-foreground">Select a row to read its value.</p>
          </Panel>
        ) : (
          <SettingDetail key={selected.name} setting={selected} onDeleted={() => setSelectedName(null)} />
        )}
      </div>

      <SettingDialog open={creating} onClose={() => setCreating(false)} setting={null} />
    </div>
  )
}

function SettingDetail({ setting, onDeleted }: { setting: ConsoleSetting; onDeleted: () => void }) {
  const [editing, setEditing] = useState(false)
  const [deleting, setDeleting] = useState(false)

  return (
    <Panel
      title={setting.name}
      description={formatTime(setting.update_time)}
      actions={
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
      }
    >
      <div className="p-3">
        <JsonBlock value={setting.value} />
      </div>

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
    </Panel>
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
    <Modal
      open={open}
      title={replacing ? `Edit ${setting.name}` : "Set a setting"}
      description="The name is the key; the value is the whole document."
      onClose={close}
      footer={
        <>
          <Button variant="outline" onClick={close} disabled={write.isPending}>
            Cancel
          </Button>
          <Button onClick={confirm} disabled={write.isPending}>
            {write.isPending && <Loader2 className="animate-spin" />}
            Save
          </Button>
        </>
      }
    >
      <TextInput
        label="Name"
        hint={replacing ? "The key cannot change: renaming would leave the old row behind." : undefined}
        value={name}
        onChange={setName}
        mono
        autoFocus={!replacing}
        disabled={replacing}
      />
      <TextArea label="Value" rows={6} value={value} onChange={setValue} />
      <p className="text-[11px] text-muted-foreground">
        The value is JSON: a string is quoted, a number is not.
      </p>
    </Modal>
  )
}
