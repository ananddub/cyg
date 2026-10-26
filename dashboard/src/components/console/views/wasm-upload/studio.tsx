// The WASM Studio's two halves, extracted so the standalone view and the per-game
// detail tabs share one implementation: the upload + live ABI checklist, and the
// deployment timeline with activate / roll-back / delete.
//
// The upload is `POST /v2/console/games/{id}/wasm/upload` as multipart form data
// (`file`, optional `source_code`), which the console's JSON mutation hook cannot
// carry, so this does the one raw request the studio needs — same bearer token,
// same rule that a refusal shows the server's own message. A 400 comes back with
// the ABI report and the missing export names; the checklist renders them red
// rather than hiding them in a toast.

import { useRef, useState, type ChangeEvent, type DragEvent, type ReactNode } from "react"
import { Check, FileCode2, Loader2, Rocket, TriangleAlert, Upload } from "lucide-react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ConfirmAction } from "@/components/console/actions"
import { DataTable, Mono, Panel } from "@/components/console/data-table"
import type { Column } from "@/components/console/data-table"
import { queryError, useConsoleQuery } from "@/hooks/use-console"
import { dash, formatTime } from "@/lib/format"
import { useSession } from "@/lib/session"

import {
  gameWasmPath,
  type AbiReport,
  type WasmModuleInfo,
} from "@/components/console/views/games"

/** The four WebAssembly magic bytes, checked client-side so an obviously wrong
 * file fails here instead of costing an upload and a server refusal. */
async function hasWasmMagic(file: File): Promise<boolean> {
  const head = new Uint8Array(await file.slice(0, 4).arrayBuffer())
  return head[0] === 0x00 && head[1] === 0x61 && head[2] === 0x73 && head[3] === 0x6d
}

function fmtBytes(value: number): string {
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`
  return `${(value / (1024 * 1024)).toFixed(2)} MiB`
}

/** What the studio holds after a send: either the accepted module or the ABI
 * report the server refused it with, so both render the same checklist. */
interface Outcome {
  ok: boolean
  abi: AbiReport
  message?: string
}

function CheckItem({
  ok,
  label,
  detail,
}: {
  ok: boolean
  label: string
  detail?: ReactNode
}) {
  return (
    <li className="flex items-start gap-2 text-sm">
      {ok ? (
        <Check className="mt-0.5 size-4 shrink-0 text-primary" />
      ) : (
        <TriangleAlert className="mt-0.5 size-4 shrink-0 text-destructive" />
      )}
      <span className="min-w-0">
        <span className={ok ? "text-foreground" : "text-destructive"}>{label}</span>
        {detail !== undefined && <div className="mt-1 text-xs text-muted-foreground">{detail}</div>}
      </span>
    </li>
  )
}

/** The three questions the console asks a module, read off the ABI report.
 * Linker compatibility is not its own probe — it is what `valid` means once the
 * bytecode and the exports already check out. */
function AbiChecklist({ abi }: { abi: AbiReport }) {
  return (
    <ul className="space-y-2.5">
      <CheckItem ok={abi.wasm_magic_ok} label="WebAssembly bytecode valid" />
      <CheckItem
        ok={abi.exports_missing.length === 0 && abi.valid}
        label={`ABI exports verified (${abi.export_count} found)`}
        detail={
          <div className="space-y-1">
            {abi.exports_found.length > 0 && (
              <div className="flex flex-wrap gap-1">
                {abi.exports_found.map((name) => (
                  <Badge key={name} variant="outline">
                    {name}
                  </Badge>
                ))}
              </div>
            )}
            {abi.exports_missing.map((name) => (
              <div key={name} className="font-mono text-destructive">
                missing export: {name}
              </div>
            ))}
          </div>
        }
      />
      <CheckItem ok={abi.valid} label="Rapier3D host linker compatible" />
    </ul>
  )
}

// ------------------------------------------------------------------- upload

/** The drop zone, optional source, progress bar and the ABI checklist the send
 * produced. Disabled entirely until a wasm-mode game is targeted. */
export function WasmUploadPanel({ gameId, disabled = false }: { gameId: string; disabled?: boolean }) {
  const token = useSession()?.token ?? ""
  const [binary, setBinary] = useState<File | null>(null)
  const [sourceName, setSourceName] = useState<string | null>(null)
  const sourceText = useRef<string>("")
  const [dragActive, setDragActive] = useState(false)
  const [progress, setProgress] = useState<number | null>(null)
  const [outcome, setOutcome] = useState<Outcome | null>(null)
  const binaryInput = useRef<HTMLInputElement>(null)
  const sourceInput = useRef<HTMLInputElement>(null)

  const modules = useConsoleQuery<WasmModuleInfo[]>(
    `game-wasm-${gameId}`,
    gameWasmPath(gameId),
    {},
    !disabled && gameId !== "",
  )

  async function acceptBinary(file: File) {
    setOutcome(null)
    if (!file.name.toLowerCase().endsWith(".wasm")) {
      toast.error("The module must be a .wasm binary.")
      return
    }
    if (!(await hasWasmMagic(file))) {
      toast.error("That file is not WebAssembly bytecode (magic bytes missing).")
      return
    }
    setBinary(file)
  }

  function onDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault()
    setDragActive(false)
    const dropped = event.dataTransfer.files[0]
    if (dropped !== undefined) void acceptBinary(dropped)
  }

  async function chooseSource(event: ChangeEvent<HTMLInputElement>) {
    const chosen = event.target.files?.[0]
    if (chosen === undefined) return
    sourceText.current = await chosen.text()
    setSourceName(chosen.name)
  }

  /** The one raw request the studio makes: multipart with upload progress, the
   * console's bearer token, and a refusal body parsed for its ABI report. */
  function upload() {
    if (disabled || binary === null || gameId === "" || token === "") return
    const form = new FormData()
    form.append("file", binary)
    if (sourceText.current !== "") form.append("source_code", sourceText.current)

    const xhr = new XMLHttpRequest()
    xhr.open("POST", `${gameWasmPath(gameId)}/upload`)
    xhr.setRequestHeader("authorization", `Bearer ${token}`)
    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable) setProgress(Math.round((event.loaded / event.total) * 100))
    }
    xhr.onload = () => {
      setProgress(null)
      let body: { module?: WasmModuleInfo; abi?: AbiReport; error?: { message?: string } } = {}
      try {
        body = JSON.parse(xhr.responseText) as typeof body
      } catch {
        // A non-JSON refusal still needs a line on screen.
      }
      const message = body.error?.message ?? `Upload refused (${xhr.status}).`
      if (xhr.status >= 200 && xhr.status < 300 && body.abi !== undefined) {
        setOutcome({ ok: true, abi: body.abi })
        toast.success(`Module v${body.module?.version ?? "?"} deployed.`)
        setBinary(null)
        setSourceName(null)
        sourceText.current = ""
        if (binaryInput.current !== null) binaryInput.current.value = ""
        if (sourceInput.current !== null) sourceInput.current.value = ""
        void modules.refetch()
      } else if (body.abi !== undefined) {
        setOutcome({ ok: false, abi: body.abi, message })
        toast.error(message)
      } else {
        setOutcome(null)
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

  return (
    <Panel
      title="Upload module"
      description="A .wasm binary; optionally its .ts source, stored alongside for audit."
    >
      <div className="space-y-3 px-4 py-3">
        <div
          role="button"
          tabIndex={0}
          onClick={() => !disabled && binaryInput.current?.click()}
          onKeyDown={(event) => event.key === "Enter" && !disabled && binaryInput.current?.click()}
          onDragOver={(event) => {
            event.preventDefault()
            setDragActive(true)
          }}
          onDragLeave={() => setDragActive(false)}
          onDrop={onDrop}
          className={`flex flex-col items-center justify-center gap-1 rounded-lg border-2 border-dashed px-4 py-8 text-center transition-colors ${
            disabled
              ? "cursor-not-allowed border-border/40 opacity-60"
              : dragActive
                ? "cursor-pointer border-primary bg-primary/5"
                : "cursor-pointer border-border hover:border-primary/50"
          }`}
        >
          <Upload className="size-5 text-muted-foreground" />
          <span className="text-sm font-medium text-foreground">
            {binary === null ? "Drop the .wasm here, or click to choose" : binary.name}
          </span>
          <span className="text-xs text-muted-foreground">
            {binary === null
              ? "Checked against the WebAssembly magic bytes before it is sent."
              : `${fmtBytes(binary.size)} · staging${sourceName === null ? "" : " · source attached"}`}
          </span>
        </div>
        <input
          ref={binaryInput}
          type="file"
          accept=".wasm,application/wasm"
          className="hidden"
          onChange={(event) => {
            const chosen = event.target.files?.[0]
            if (chosen !== undefined) void acceptBinary(chosen)
          }}
        />

        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <FileCode2 className="size-4" />
          {sourceName === null ? "Module source (.ts, optional)" : sourceName}
          <input
            ref={sourceInput}
            type="file"
            accept=".ts,text/plain"
            onChange={chooseSource}
            className="text-xs text-muted-foreground"
          />
        </label>

        {progress !== null && (
          <div className="space-y-1">
            <div className="h-1.5 overflow-hidden rounded-full bg-secondary">
              <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${progress}%` }} />
            </div>
            <span className="font-mono text-xs text-muted-foreground">{progress}%</span>
          </div>
        )}

        <div className="flex justify-end">
          <Button onClick={upload} disabled={disabled || binary === null || gameId === "" || progress !== null}>
            {progress === null ? <Rocket /> : <Loader2 className="animate-spin" />}
            Upload &amp; validate
          </Button>
        </div>

        {outcome !== null && (
          <div className="space-y-2 rounded-md border border-border/60 bg-background/60 p-3">
            <p className="text-xs font-medium text-muted-foreground">
              {outcome.ok ? "ABI validation passed" : outcome.message}
            </p>
            <AbiChecklist abi={outcome.abi} />
          </div>
        )}
      </div>
    </Panel>
  )
}

// ------------------------------------------------------------ deployment list

/** The deployed versions and their activate / roll-back / delete controls. */
export function DeploymentHistory({ gameId, disabled = false }: { gameId: string; disabled?: boolean }) {
  const [deploying, setDeploying] = useState<WasmModuleInfo | null>(null)
  const [removing, setRemoving] = useState<WasmModuleInfo | null>(null)

  const modules = useConsoleQuery<WasmModuleInfo[]>(
    `game-wasm-${gameId}`,
    gameWasmPath(gameId),
    {},
    !disabled && gameId !== "",
  )

  const columns: Column<WasmModuleInfo>[] = [
    {
      header: "Version",
      cell: (row) => (
        <span className="flex items-center gap-2">
          <span className="font-medium text-foreground">v{row.version}</span>
          {row.is_active && <Badge>Active</Badge>}
        </span>
      ),
    },
    { header: "File", cell: (row) => <span className="text-xs text-foreground">{dash(row.filename)}</span> },
    {
      header: "SHA-256",
      cell: (row) => (
        <span title={row.sha256}>
          <Mono>{`${row.sha256.slice(0, 12)}…`}</Mono>
        </span>
      ),
    },
    { header: "Size", cell: (row) => <Mono>{fmtBytes(row.size_bytes)}</Mono> },
    { header: "Source", cell: (row) => (row.has_source ? "attached" : <Mono>—</Mono>) },
    { header: "Uploaded", cell: (row) => <Mono>{formatTime(row.created_at)}</Mono> },
    {
      header: "",
      cell: (row) => (
        <div className="flex justify-end gap-1.5" onClick={(event) => event.stopPropagation()}>
          {!row.is_active && (
            <Button size="xs" variant="outline" onClick={() => setDeploying(row)}>
              {row.version < (modules.data?.find((m) => m.is_active)?.version ?? 0)
                ? `Rollback to v${row.version}`
                : `Activate v${row.version}`}
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
      <Panel
        title="Deployment timeline"
        description={
          modules.data === undefined ? undefined : `${modules.data.length} versions · newest first`
        }
        fill
      >
        <DataTable
          columns={columns}
          rows={modules.data}
          loading={!disabled && gameId !== "" && modules.isPending}
          error={!disabled && modules.data === undefined ? queryError(modules.error) : null}
          rowKey={(row) => String(row.version)}
          empty="No modules deployed for this game yet."
          fill
        />
      </Panel>

      {/* Activate and roll back are the same write — the runtime swaps to whichever
          version is named — so one confirm carries both labels. */}
      <ConfirmAction
        open={deploying !== null}
        onClose={() => setDeploying(null)}
        title={deploying === null ? "" : `Activate v${deploying.version}`}
        message={
          deploying === null
            ? ""
            : `The runtime will serve v${deploying.version} (${deploying.filename}) instead of the version it holds now. The previous module stays deployed.`
        }
        confirmLabel={deploying === null ? "Activate" : `Activate v${deploying.version}`}
        method="POST"
        path={`${gameWasmPath(gameId)}/activate`}
        body={deploying === null ? {} : { version: deploying.version }}
        success={`v${deploying?.version ?? ""} is live.`}
      />

      <ConfirmAction
        open={removing !== null}
        onClose={() => setRemoving(null)}
        title={removing === null ? "" : `Delete v${removing.version}`}
        message={
          removing === null
            ? ""
            : "The version and its stored module are removed. The active version cannot be deleted, and this cannot be undone."
        }
        confirmLabel="Delete version"
        destructive
        method="DELETE"
        path={removing === null ? gameWasmPath(gameId) : `${gameWasmPath(gameId)}/${removing.version}`}
        success={`v${removing?.version ?? ""} deleted.`}
      />
    </>
  )
}
