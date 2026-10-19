// Small pieces shared by the Games Catalog view, its register modal and its
// detail sheet. They live apart so the detail and the catalog can both render
// them without either importing the other (no render-only cycle).

import { useState } from "react"
import { Check, Copy, Eye, EyeOff } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Mono } from "@/components/console/data-table"
import { FormSheet } from "@/components/console/form-sheet"
import { dash } from "@/lib/format"

import { engineLabel, modeBadge } from "./options"
import type { BackendMode } from "./routes"

/** A value with a one-click copy affordance, for keys an operator pastes into a
 * game client's config. */
export function CopyValue({ value, className }: { value: string; className?: string }) {
  const [copied, setCopied] = useState(false)

  function copy(event: React.MouseEvent) {
    event.stopPropagation()
    navigator.clipboard
      ?.writeText(value)
      .then(() => {
        setCopied(true)
        window.setTimeout(() => setCopied(false), 1500)
      })
      .catch(() => setCopied(false))
  }

  return (
    <button
      type="button"
      onClick={copy}
      title="Copy"
      className={`inline-flex items-center gap-1 text-left hover:text-foreground ${className ?? ""}`}
    >
      <span className="truncate">{value}</span>
      {copied ? (
        <Check className="size-3 shrink-0 text-primary" />
      ) : (
        <Copy className="size-3 shrink-0 opacity-50" />
      )}
    </button>
  )
}

/** The engine name plus the backend-mode the server tagged it with, so a row
 * reads either "…Relay" or "…WASM" at a glance. */
export function EngineBadge({
  engine,
  mode,
}: {
  engine: string
  mode: BackendMode | undefined
}) {
  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      <Badge variant="secondary">{engineLabel(engine)}</Badge>
      {mode === "relay" && (
        <Badge variant="outline" title={modeBadge(mode, engine)}>
          Relay
        </Badge>
      )}
      {mode === "wasm" && (
        <Badge variant="outline" title={modeBadge(mode, engine)}>
          WASM
        </Badge>
      )}
    </span>
  )
}

export function ModePill({ mode }: { mode: BackendMode | undefined }) {
  if (mode === "relay") return <Badge variant="secondary">C# Relay</Badge>
  if (mode === "wasm") return <Badge variant="secondary">Server WASM</Badge>
  return <Badge variant="outline">{dash(mode)}</Badge>
}

export function StatusPill({ status }: { status: string }) {
  if (status === "active") return <Badge>Active</Badge>
  if (status === "suspended") return <Badge variant="destructive">Suspended</Badge>
  return <Badge variant="outline">{dash(status)}</Badge>
}

/** The API key dialog: masked until revealed, copyable once shown, and stamped
 * with the fact that the creation key is never repeated. Reopened from a game's
 * detail, it re-reads the stored key through `GET /v2/console/games/{id}`. */
export function ApiKeyDialog({
  gameName,
  apiKey,
  onClose,
}: {
  gameName: string
  apiKey: string
  onClose: () => void
}) {
  const [revealed, setRevealed] = useState(false)

  return (
    <FormSheet
      open
      title="Game API key"
      description={gameName}
      onClose={onClose}
      footer={<Button onClick={onClose}>Done</Button>}
    >
      <p className="text-sm text-muted-foreground">
        This key is shown once and is not repeated by any list. Store it now; it can only be
        re-read later through the game&rsquo;s detail view, on demand.
      </p>
      <div className="flex items-center gap-2 rounded-md border border-border bg-background/60 p-2">
        <Mono className="min-w-0 flex-1 break-all text-foreground">
          {revealed ? apiKey : "••••••••••••••••••••••••"}
        </Mono>
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={() => setRevealed((r) => !r)}
          aria-label={revealed ? "Hide" : "Reveal"}
        >
          {revealed ? <EyeOff /> : <Eye />}
        </Button>
        {revealed && (
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => void navigator.clipboard?.writeText(apiKey)}
            aria-label="Copy"
          >
            <Copy />
          </Button>
        )}
      </div>
    </FormSheet>
  )
}
