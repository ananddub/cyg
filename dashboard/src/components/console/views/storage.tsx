// The storage browser: collections on the left as the rail, objects in the
// selected one as the table, and the object's value in the drawer — the same
// right-side sheet every other detail uses.

import { useState } from "react"
import { Trash2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, Field, JsonBlock, Mono, PageHeading, Panel, SearchField } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { DetailSheet } from "@/components/console/detail-sheet"
import { useConsoleQuery, queryError } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS, type ConsoleStorageObject } from "@/lib/console"
import { formatTime, shortId } from "@/lib/format"
import { cn } from "@/lib/utils"

const PERMISSION = ["private", "owner", "public"] as const

function permission(level: number): string {
  return PERMISSION[level] ?? String(level)
}

/** Storage rows are addressed by three parts at once, so the composite is the
 * only honest row key. */
function objectKey(row: ConsoleStorageObject): string {
  return `${row.collection}/${row.key}/${row.user_id}`
}

export function StorageView() {
  const collections = useConsoleQuery<string[]>("storage-collections", CONSOLE_ENDPOINTS.storageCollections)
  const [selected, setSelected] = useState<string | null>(null)
  const [openKey, setOpenKey] = useState<string | null>(null)
  const [deletingCollection, setDeletingCollection] = useState<string | null>(null)
  const [deletingObject, setDeletingObject] = useState(false)
  const [query, setQuery] = useState("")

  const objects = useConsoleQuery<ConsoleStorageObject[]>(
    "storage-objects",
    selected === null
      ? CONSOLE_ENDPOINTS.storage
      : `${CONSOLE_ENDPOINTS.storage}/${encodeURIComponent(selected)}`,
  )

  // Read back from the list, so deleting the opened object — or switching
  // collections — closes the drawer instead of pinning a stale value to it.
  const opened = objects.data?.find((row) => objectKey(row) === openKey) ?? null

  const needle = query.trim().toLowerCase()
  const filtered = (objects.data ?? []).filter((row) => {
    if (needle === "") return true
    return [row.key, row.collection, row.user_id, row.version].some((value) =>
      value.toLowerCase().includes(needle),
    )
  })

  const empty =
    objects.data !== undefined && objects.data.length > 0
      ? "No objects match this search."
      : "No objects."

  const columns: Column<ConsoleStorageObject>[] = [
    { header: "Key", cell: (row) => <span className="font-medium text-foreground">{row.key}</span> },
    { header: "Owner", cell: (row) => <Mono>{shortId(row.user_id)}</Mono> },
    { header: "Read", cell: (row) => <Mono>{permission(row.read)}</Mono> },
    { header: "Write", cell: (row) => <Mono>{permission(row.write)}</Mono> },
    { header: "Version", cell: (row) => <Mono>{shortId(row.version)}</Mono> },
    { header: "Updated", cell: (row) => <Mono>{formatTime(row.update_time)}</Mono> },
  ]

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="Storage"
        description="Every stored object, including private ones the player storage API hides."
      />

      <SearchField
        value={query}
        onChange={setQuery}
        placeholder="Search objects by key, collection or owner"
        label="Search storage objects"
      />

      <div className="grid min-h-0 flex-1 gap-6 lg:grid-cols-[14rem_minmax(0,1fr)]">
        <Panel
          title="Collections"
          description={`${collections.data?.length ?? 0} total`}
          className="min-h-0"
          fill
        >
          {collections.data === undefined || collections.data.length === 0 ? (
            <p className="flex flex-1 items-center justify-center px-4 py-6 text-center text-sm text-muted-foreground">
              None yet.
            </p>
          ) : (
            <div className="min-h-0 flex-1 overflow-auto py-1">
              <button
                type="button"
                onClick={() => {
                  setSelected(null)
                  setOpenKey(null)
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
                      setOpenKey(null)
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

        <Panel className="min-h-0" fill>
          <DataTable
            columns={columns}
            rows={filtered}
            loading={objects.isPending}
            error={queryError(objects.error)}
            rowKey={objectKey}
            onRowClick={(row) => setOpenKey(objectKey(row))}
            selectedKey={openKey ?? undefined}
            empty={empty}
            fill
            selectable
          />
        </Panel>
      </div>

      <DetailSheet
        open={opened !== null}
        onClose={() => setOpenKey(null)}
        title={opened?.key ?? "Object"}
        description={opened ? `${opened.collection} · ${shortId(opened.user_id)}` : undefined}
        bodyClassName="space-y-4"
      >
        {opened !== null && (
          <>
            <div>
              <Field label="Read">{permission(opened.read)}</Field>
              <Field label="Write">{permission(opened.write)}</Field>
              <Field label="Version">{opened.version}</Field>
              <Field label="Updated">{formatTime(opened.update_time)}</Field>
            </div>
            <JsonBlock value={opened.value} />
            <Button
              variant="destructive"
              size="sm"
              onClick={() => setDeletingObject(true)}
            >
              <Trash2 />
              Delete object
            </Button>
            <ConfirmAction
              open={deletingObject}
              onClose={() => setDeletingObject(false)}
              onDone={() => setOpenKey(null)}
              title="Delete object"
              message={`${opened.collection}/${opened.key}, owned by ${opened.user_id}.`}
              confirmLabel="Delete object"
              destructive
              method="DELETE"
              path={`${CONSOLE_ENDPOINTS.storageObject}/${encodeURIComponent(opened.collection)}/${encodeURIComponent(opened.key)}/${opened.user_id}`}
              success="Object deleted."
            />
          </>
        )}
      </DetailSheet>
    </div>
  )
}
