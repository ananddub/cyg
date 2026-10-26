// A game's map assets: the upload panel (cooked mesh / arena JSON / navmesh)
// and the version table with activate and delete. Rendered by the per-game
// detail tabs as the "Maps" tab.
//
// The upload is `POST /v2/console/games/{id}/maps/upload` as multipart form
// data (`file`, `format`, optional `name`) — like the wasm studio, the one raw
// request the panel needs, same bearer token, refusals surfaced as toasts.

import { useRef, useState, type ChangeEvent, type DragEvent } from "react"
import { Loader2, Map as MapIcon, Rocket, Upload } from "lucide-react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, Mono, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { queryError, useConsoleQuery } from "@/hooks/use-console"
import { dash, formatTime } from "@/lib/format"
import { useSession } from "@/lib/session"

import { gameMapsPath } from "./routes"

/** One stored `game_maps` row, without its bytes. */
export interface GameMapInfo {
  id: string
  version: number
  name: string
  format: "oxcl" | "map2d" | "map3d" | "navmesh"
  sha256: string
  is_active: boolean
  size_bytes: number
  created_at: string
}

const FORMAT_LABELS: Record<GameMapInfo["format"], string> = {
  oxcl: "3D mesh (OXCL)",
  map2d: "2D arena (JSON)",
  map3d: "3D arena (JSON)",
  navmesh: "Navmesh",
}

/** `format` per extension, mirroring the server's inference so the field the
 * upload sends always matches what the file actually is. */
const EXT_FORMATS: Record<string, GameMapInfo["format"]> = {
  oxcl: "oxcl",
  json: "map2d",
  nav2d: "navmesh",
  mesh: "navmesh",
}

function fmtBytes(value: number): string {
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`
  return `${(value / (1024 * 1024)).toFixed(2)} MiB`
}

function inferFormat(name: string): GameMapInfo["format"] | null {
  const ext = name.split(".").pop()?.toLowerCase() ?? ""
  return EXT_FORMATS[ext] ?? null
}

/** The drop zone + format picker. A `.json` upload needs its dialect picked
 * (2D arena vs 3D arena); the other extensions carry their format alone. */
export function MapsUploadPanel({ gameId }: { gameId: string }) {
  const token = useSession()?.token ?? ""
  const [file, setFile] = useState<File | null>(null)
  const [format, setFormat] = useState<GameMapInfo["format"]>("map2d")
  const [dragActive, setDragActive] = useState(false)
  const [progress, setProgress] = useState<number | null>(null)
  const fileInput = useRef<HTMLInputElement>(null)

  const maps = useConsoleQuery<GameMapInfo[]>(`game-maps-${gameId}`, gameMapsPath(gameId), {}, gameId !== "")

  async function accept(file: File) {
    setFile(file)
    const inferred = inferFormat(file.name)
    if (inferred !== null) setFormat(inferred)
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault()
    setDragActive(false)
    const dropped = event.dataTransfer.files[0]
    if (dropped !== undefined) void accept(dropped)
  }

  function onPick(event: ChangeEvent<HTMLInputElement>) {
    const chosen = event.target.files?.[0]
    if (chosen !== undefined) void accept(chosen)
  }

  /** The one raw request the panel makes: multipart with upload progress. */
  function upload() {
    if (file === null || gameId === "" || token === "") return
    const form = new FormData()
    form.append("file", file)
    form.append("format", format)

    const xhr = new XMLHttpRequest()
    xhr.open("POST", `${gameMapsPath(gameId)}/upload`)
    xhr.setRequestHeader("authorization", `Bearer ${token}`)
    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable) setProgress(Math.round((event.loaded / event.total) * 100))
    }
    xhr.onload = () => {
      setProgress(null)
      let body: { map?: GameMapInfo; error?: { message?: string } } = {}
      try {
        body = JSON.parse(xhr.responseText) as typeof body
      } catch {
        // A non-JSON refusal still needs a line on screen.
      }
      const message = body.error?.message ?? `Upload refused (${xhr.status}).`
      if (xhr.status >= 200 && xhr.status < 300 && body.map !== undefined) {
        toast.success(`Map v${body.map.version} uploaded and active.`)
        setFile(null)
        if (fileInput.current !== null) fileInput.current.value = ""
        void maps.refetch()
      } else {
        toast.error(message)
      }
    }
    xhr.onerror = () => {
      setProgress(null)
      toast.error("The upload failed before the server answered.")
    }
    setProgress(0)
    xhr.send(form)
  }

  const formatOptions = Object.entries(FORMAT_LABELS) as [GameMapInfo["format"], string][]

  return (
    <Panel
      title="Upload map"
      description="A cooked .oxcl mesh, an arena JSON (2D/3D), or a baked navmesh — the server loads the active version at boot."
    >
      <div className="space-y-3 px-4 py-3">
        <div
          role="button"
          tabIndex={0}
          onClick={() => fileInput.current?.click()}
          onKeyDown={(event) => event.key === "Enter" && fileInput.current?.click()}
          onDragOver={(event) => {
            event.preventDefault()
            setDragActive(true)
          }}
          onDragLeave={() => setDragActive(false)}
          onDrop={onDrop}
          className={`flex flex-col items-center justify-center gap-1 rounded-lg border-2 border-dashed px-4 py-8 text-center transition-colors ${
            dragActive
              ? "cursor-pointer border-primary bg-primary/5"
              : "cursor-pointer border-border hover:border-primary/50"
          }`}
        >
          <Upload className="size-5 text-muted-foreground" />
          <span className="text-sm font-medium text-foreground">
            {file === null ? "Drop the map here, or click to choose" : file.name}
          </span>
          <span className="text-xs text-muted-foreground">
            {file === null
              ? ".oxcl · .json (2D/3D arena) · .nav2d/.mesh (navmesh) — up to 32 MiB"
              : `${fmtBytes(file.size)} · ${FORMAT_LABELS[format]}`}
          </span>
        </div>
        <input
          ref={fileInput}
          type="file"
          accept=".oxcl,.json,.nav2d,.mesh"
          className="hidden"
          onChange={onPick}
        />

        <div className="flex flex-wrap items-center gap-3">
          <span className="text-xs font-medium text-muted-foreground">Format</span>
          <div className="flex flex-wrap gap-1.5">
            {formatOptions.map(([value, label]) => (
              <button
                key={value}
                type="button"
                onClick={() => setFormat(value)}
                className={`rounded-md border px-2 py-1 text-xs transition-colors ${
                  format === value
                    ? "border-primary bg-primary/10 text-foreground"
                    : "border-border text-muted-foreground hover:border-primary/40 hover:text-foreground"
                }`}
              >
                {label}
              </button>
            ))}
          </div>
        </div>

        {progress !== null && (
          <div className="space-y-1">
            <div className="h-1.5 overflow-hidden rounded-full bg-secondary">
              <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${progress}%` }} />
            </div>
            <span className="font-mono text-xs text-muted-foreground">{progress}%</span>
          </div>
        )}

        <div className="flex justify-end">
          <Button onClick={upload} disabled={file === null || gameId === "" || progress !== null}>
            {progress === null ? <Rocket /> : <Loader2 className="animate-spin" />}
            Upload &amp; activate
          </Button>
        </div>
      </div>
    </Panel>
  )
}

/** The stored versions and their activate / delete controls. */
export function MapsPanel({ gameId }: { gameId: string }) {
  const [activating, setActivating] = useState<GameMapInfo | null>(null)
  const [removing, setRemoving] = useState<GameMapInfo | null>(null)

  const maps = useConsoleQuery<GameMapInfo[]>(`game-maps-${gameId}`, gameMapsPath(gameId), {}, gameId !== "")

  const columns: Column<GameMapInfo>[] = [
    {
      header: "Version",
      cell: (row) => (
        <span className="flex items-center gap-2">
          <span className="font-medium text-foreground">v{row.version}</span>
          {row.is_active && <Badge>Active</Badge>}
        </span>
      ),
    },
    { header: "Name", cell: (row) => <span className="text-xs text-foreground">{dash(row.name)}</span> },
    {
      header: "Format",
      cell: (row) => (
        <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <MapIcon className="size-3.5" />
          {FORMAT_LABELS[row.format] ?? row.format}
        </span>
      ),
    },
    { header: "Size", cell: (row) => <Mono>{fmtBytes(row.size_bytes)}</Mono> },
    {
      header: "SHA-256",
      cell: (row) => (
        <span title={row.sha256}>
          <Mono>{`${row.sha256.slice(0, 12)}…`}</Mono>
        </span>
      ),
    },
    { header: "Uploaded", cell: (row) => <Mono>{formatTime(row.created_at)}</Mono> },
    {
      header: "",
      cell: (row) => (
        <div className="flex justify-end gap-1.5" onClick={(event) => event.stopPropagation()}>
          {!row.is_active && (
            <Button size="xs" variant="outline" onClick={() => setActivating(row)}>
              Activate v{row.version}
            </Button>
          )}
          {!row.is_active && (
            <Button size="xs" variant="ghost" onClick={() => setRemoving(row)}>
              Delete
            </Button>
          )}
        </div>
      ),
    },
  ]

  return (
    <>
      <Panel fill>
        <DataTable
          columns={columns}
          rows={maps.data ?? []}
          loading={gameId !== "" && maps.isPending}
          error={maps.data === undefined ? queryError(maps.error) : null}
          rowKey={(row) => String(row.version)}
          empty="No maps uploaded for this game yet — the server falls back to its project folder's local map files."
          fill
        />
      </Panel>

      <ConfirmAction
        open={activating !== null}
        onClose={() => setActivating(null)}
        title={activating === null ? "" : `Activate v${activating.version}`}
        message={
          activating === null
            ? ""
            : `New rooms spawn on ${activating.name} (${FORMAT_LABELS[activating.format]}). The version the server holds now stays stored, and a restart re-loads this choice either way.`
        }
        confirmLabel="Activate"
        method="POST"
        path={`${gameMapsPath(gameId)}/${activating?.version ?? 0}/activate`}
        success={`v${activating?.version ?? ""} is the active map.`}
      />

      <ConfirmAction
        open={removing !== null}
        onClose={() => setRemoving(null)}
        title={removing === null ? "" : `Delete v${removing.version}`}
        message="The version and its stored map are removed. The active version cannot be deleted, and this cannot be undone."
        confirmLabel="Delete version"
        destructive
        method="DELETE"
        path={removing === null ? gameMapsPath(gameId) : `${gameMapsPath(gameId)}/${removing.version}`}
        success={`v${removing?.version ?? ""} deleted.`}
      />
    </>
  )
}
