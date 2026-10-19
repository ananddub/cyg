// ACL templates: named permission documents an operator's role can be named from.
// The document is the same shape `console_user.acl` holds, so a template is
// exactly what an operator row would carry.

import { useState } from "react"
import { Pencil, Plus, Trash2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, JsonBlock, Mono, PageHeading, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { DetailSheet } from "@/components/console/detail-sheet"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleAclTemplate } from "@/lib/console"
import { dash, formatTime, shortId } from "@/lib/format"

import { TemplateDialog } from "./acl-dialog"

const ACL = CONSOLE_ENDPOINTS.aclTemplates

export function AclTemplatesView() {
  const list = useConsoleQuery<ConsoleAclTemplate[]>("acl", ACL)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [creating, setCreating] = useState(false)

  const selected = list.data?.find((template) => template.id === selectedId) ?? null

  const columns: Column<ConsoleAclTemplate>[] = [
    { header: "Name", cell: (row) => <span className="font-medium text-foreground">{row.name}</span> },
    { header: "Description", cell: (row) => dash(row.description) },
    { header: "Updated", cell: (row) => <Mono>{formatTime(row.update_time)}</Mono> },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="ACL Templates"
        description="Named permission documents an operator role can be defined from."
      />

      <Panel
        title="Templates"
        fill
        actions={
          <Button size="sm" variant="outline" onClick={() => setCreating(true)}>
            <Plus />
            New template
          </Button>
        }
      >
        <DataTable
          columns={columns}
          rows={list.data}
          loading={list.isPending}
          error={queryError(list.error)}
          rowKey={(row) => row.id}
          onRowClick={(row) => setSelectedId(row.id)}
          selectedKey={selectedId ?? undefined}
          empty="No ACL templates defined."
          fill
          selectable
        />
      </Panel>

      <DetailSheet
        open={selected !== null}
        onClose={() => setSelectedId(null)}
        title={selected?.name ?? "Permissions"}
        description={selected ? shortId(selected.id) : undefined}
      >
        {selected !== null && (
          <TemplateDetail key={selected.id} template={selected} onDeleted={() => setSelectedId(null)} />
        )}
      </DetailSheet>

      <TemplateDialog open={creating} onClose={() => setCreating(false)} template={null} />
    </div>
  )
}

function TemplateDetail({
  template,
  onDeleted,
}: {
  template: ConsoleAclTemplate
  onDeleted: () => void
}) {
  const [editing, setEditing] = useState(false)
  const [deleting, setDeleting] = useState(false)

  return (
    <Panel
      title={template.name}
      description={shortId(template.id)}
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
      {template.description !== "" && (
        <p className="px-4 pt-3 text-xs text-muted-foreground">{template.description}</p>
      )}
      <div className="p-3">
        <JsonBlock value={template.acl} />
      </div>

      <TemplateDialog open={editing} onClose={() => setEditing(false)} template={template} />
      <ConfirmAction
        open={deleting}
        onClose={() => setDeleting(false)}
        onDone={onDeleted}
        title="Delete template"
        message="Operators already created from it keep the document they carry; only the template goes."
        confirmLabel="Delete"
        destructive
        method="DELETE"
        path={`${ACL}/${template.id}`}
        success="Template deleted."
      />
    </Panel>
  )
}
