// The storage browser: collections, the objects in the selected one, and the
// selected object's value.

import { useState } from "react"
import { Trash2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, JsonBlock, Mono, PageHeading, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleStorageObject } from "@/lib/console"
import { formatTime, shortId } from "@/lib/format"
import { cn } from "@/lib/utils"

const PERMISSION = ["private", "owner", "public"] as const

function permission(level: number): string {
  return PERMISSION[level] ?? String(level)
}

export function StorageView() {
  const collections = useConsoleQuery<string[]>("storage-collections", CONSOLE_ENDPOINTS.storageCollections)
  const [selected, setSelected] = useState<string | null>(null)
  const [object, setObject] = useState<ConsoleStorageObject | null>(null)
  const [deletingCollection, setDeletingCollection] = useState<string | null>(null)
  const [deletingObject, setDeletingObject] = useState(false)

  const objects = useConsoleQuery<ConsoleStorageObject[]>(
    "storage-objects",
    selected === null
      ? CONSOLE_ENDPOINTS.storage
      : `${CONSOLE_ENDPOINTS.storage}/${encodeURIComponent(selected)}`,
  )

  const columns: Column<ConsoleStorageObject>[] = [
    { header: "Key", cell: (row) => <span className="font-medium text-foreground">{row.key}</span> },
    { header: "Owner", cell: (row) => <Mono>{shortId(row.user_id)}</Mono> },
    { header: "Read", cell: (row) => <Mono>{permission(row.read)}</Mono> },
    { header: "Write", cell: (row) => <Mono>{permission(row.write)}</Mono> },
    { header: "Version", cell: (row) => <Mono>{shortId(row.version)}</Mono> },
    { header: "Updated", cell: (row) => <Mono>{formatTime(row.update_time)}</Mono> },
  ]

  return (
    <div className="space-y-6">
      <PageHeading
        title="Storage"
        description="Every stored object, including private ones the player storage API hides."
      />

      <div className="grid gap-6 lg:grid-cols-[14rem_minmax(0,1fr)]">
        <Panel title="Collections" description={`${collections.data?.length ?? 0} total`}>
          {collections.data === undefined || collections.data.length === 0 ? (
            <p className="px-4 py-6 text-sm text-muted-foreground">None yet.</p>
          ) : (
            <div className="max-h-[32rem] overflow-auto py-1">
              <button
                type="button"
                onClick={() => {
                  setSelected(null)
                  setObject(null)
                }}
                className={cn(
                  "block w-full px-4 py-1.5 text-left text-xs transition-colors hover:bg-foreground/5",
                  selected === null ? "text-primary" : "text-muted-foreground",
                )}
              >
                All objects
              </button>
              {collections.data.map((name) => (
                <div key={name} className="flex items-center">
                  <button
                    type="button"
                    onClick={() => {
                      setSelected(name)
                      setObject(null)
                    }}
                    className={cn(
                      "min-w-0 flex-1 truncate px-4 py-1.5 text-left font-mono text-xs transition-colors hover:bg-foreground/5",
                      selected === name ? "text-primary" : "text-muted-foreground",
                    )}
                  >
                    {name}
                  </button>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    aria-label={`Delete ${name}`}
                    onClick={() => setDeletingCollection(name)}
                  >
                    <Trash2 />
                  </Button>
                </div>
              ))}
            </div>
          )}
        </Panel>

        <ConfirmAction
          open={deletingCollection !== null}
          onClose={() => setDeletingCollection(null)}
          onDone={() => {
            if (deletingCollection === selected) setSelected(null)
          }}
          title="Delete collection"
          message={`Every object in "${deletingCollection ?? ""}" goes, for every account. Nothing puts it back.`}
          confirmLabel="Delete collection"
          destructive
          method="DELETE"
          path={`${CONSOLE_ENDPOINTS.storageCollection}/${encodeURIComponent(deletingCollection ?? "")}`}
          success="Collection deleted."
        />

        <div className="space-y-4">
          <Panel title={selected ?? "All objects"}>
            <DataTable
              columns={columns}
              rows={objects.data}
              loading={objects.isPending}
              error={queryError(objects.error)}
              rowKey={(row) => `${row.collection}/${row.key}/${row.user_id}`}
              onRowClick={(row) => setObject(row)}
              selectedKey={object === null ? undefined : `${object.collection}/${object.key}/${object.user_id}`}
              empty="No objects."
            />
          </Panel>

          {object !== null && (
            <Panel
              title="Value"
              description={`${object.collection}/${object.key}`}
              actions={
                <Button size="sm" variant="destructive" onClick={() => setDeletingObject(true)}>
                  <Trash2 />
                  Delete
                </Button>
              }
            >
              <div className="p-3">
                <JsonBlock value={object.value} />
              </div>
              <ConfirmAction
                open={deletingObject}
                onClose={() => setDeletingObject(false)}
                onDone={() => setObject(null)}
                title="Delete object"
                message={`${object.collection}/${object.key}, owned by ${object.user_id}.`}
                confirmLabel="Delete object"
                destructive
                method="DELETE"
                path={`${CONSOLE_ENDPOINTS.storageObject}/${encodeURIComponent(object.collection)}/${encodeURIComponent(object.key)}/${object.user_id}`}
                success="Object deleted."
              />
            </Panel>
          )}
        </div>
      </div>
    </div>
  )
}
