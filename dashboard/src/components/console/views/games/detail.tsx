// The per-game detail sheet, split into engine-aware tabs.
//
// The server owns `backend_mode`, and the tabs mirror it exactly:
//   relay (unity/unreal/godot) → Unity SDK · Credentials & Config · Live Rooms
//   wasm  (web/custom)         → WASM Studio · Deployment History · Live Rooms
//
// For a relay game there is no WASM tab, no upload button and no deploy link —
// they are not rendered at all, so a developer working a Zero Server Code game
// never sees the plane that does not apply. The Live Rooms tab reuses the
// matches/room feed the Matches area already owns; nothing new is wired here.

import { useState } from "react"
import {
  Braces,
  Copy,
  Download,
  Eye,
  EyeOff,
  Loader2,
  Settings2,
  Trash2,
} from "lucide-react"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs"
import { ConfirmAction } from "@/components/console/actions"
import { Checkbox, Select, TextInput } from "@/components/console/form"
import { Field, Mono, Panel } from "@/components/console/data-table"
import { FormSheet } from "@/components/console/form-sheet"
import { queryError, useConsoleMutation, useConsoleQuery } from "@/hooks/use-console"
import { dash, formatTime, num } from "@/lib/format"
import { useSession } from "@/lib/session"
import {
  CONSOLE_ENDPOINTS,
  type ConsoleMatch,
  type ConsoleMatchList,
} from "@/lib/console"
import { MatchDetail } from "@/components/console/views/matches/detail"
import {
  DeploymentHistory,
  WasmUploadPanel,
} from "@/components/console/views/wasm-upload"
import { MapsPanel, MapsUploadPanel } from "./maps"

import {
  engineLabel,
  CAPACITY_OPTIONS,
  STATUS_OPTIONS,
  TICK_RATE_OPTIONS,
} from "./options"
import {
  gamePath,
  gameUnitySettingsPath,
} from "./routes"
import type { GameConfig, GameDetail, UnitySettingsJson } from "./model"
import { CopyValue, StatusPill } from "./parts"

// ------------------------------------------------------------------ helpers

function filenameFromDisposition(header: string | null, fallback: string): string {
  if (header === null) return fallback
  const star = /filename\*\s*=\s*[^;]*'[^']*'?([^;']+)'?/i.exec(header)
  const plain = /filename\s*=\s*"?([^";]+)"?/i.exec(header)
  const raw = (star ?? plain)?.[1]?.trim()
  return raw === undefined || raw === "" ? fallback : decodeURIComponent(raw)
}

function ConfigFields({ config }: { config: GameConfig }) {
  return (
    <>
      <Field label="Tick rate">{num(config.tick_rate)} Hz</Field>
      <Field label="Max players">{num(config.max_clients)}</Field>
      <Field label="Persistent rooms">{config.persistent_rooms ? "on" : "off"}</Field>
      <Field label="Auto-close">{config.persistent_rooms ? `${num(config.auto_close_seconds)} s` : "—"}</Field>
    </>
  )
}

function CredentialsTab({ game, onConfigure }: { game: GameDetail; onConfigure?: () => void }) {
  const token = useSession()?.token ?? ""
  const [revealed, setRevealed] = useState(false)
  const [downloading, setDownloading] = useState(false)
  const [copyingJson, setCopyingJson] = useState(false)

  async function downloadAsset() {
    if (token === "") return
    setDownloading(true)
    try {
      const response = await fetch(`${gameUnitySettingsPath(game.id)}?format=asset`, {
        headers: { authorization: `Bearer ${token}` },
      })
      if (!response.ok) throw new Error(`Download refused (${response.status}).`)
      const blob = await response.blob()
      const filename = filenameFromDisposition(response.headers.get("content-disposition"), "CYGServerSettings.asset")
      const url = URL.createObjectURL(blob)
      const anchor = document.createElement("a")
      anchor.href = url
      anchor.download = filename
      anchor.click()
      URL.revokeObjectURL(url)
      toast.success(`Saved ${filename}`)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "The download failed.")
    } finally {
      setDownloading(false)
    }
  }

  async function copyConnectionJson() {
    if (token === "") return
    setCopyingJson(true)
    try {
      const response = await fetch(`${gameUnitySettingsPath(game.id)}?format=json`, {
        headers: { authorization: `Bearer ${token}`, accept: "application/json" },
      })
      if (!response.ok) throw new Error(`Refused (${response.status}).`)
      const settings = (await response.json()) as UnitySettingsJson
      await navigator.clipboard?.writeText(JSON.stringify(settings, null, 2))
      toast.success("Connection JSON copied.")
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Could not read the connection JSON.")
    } finally {
      setCopyingJson(false)
    }
  }

  return (
    <div className="space-y-4">
      <Panel title="Identifiers" description="Game keys and credentials for connecting clients.">
        <div className="space-y-2 px-4 py-3">
          <div className="flex items-center justify-between gap-3 text-xs">
            <span className="text-muted-foreground">game_key</span>
            <CopyValue value={game.game_key} className="min-w-0 flex-1 justify-end font-mono text-foreground" />
          </div>
          <div className="flex items-center justify-between gap-3 text-xs">
            <span className="text-muted-foreground">client_key</span>
            <CopyValue value={dash(game.client_key)} className="min-w-0 flex-1 justify-end font-mono text-foreground" />
          </div>
          <div className="flex items-center justify-between gap-3 border-t border-border/40 pt-2 text-xs">
            <span className="text-muted-foreground">api_key</span>
            {game.api_key === undefined || game.api_key === "" ? (
              <Mono>not loaded</Mono>
            ) : (
              <div className="flex items-center gap-1.5">
                <Mono className="text-foreground">{revealed ? game.api_key : "••••••••••"}</Mono>
                <Button
                  variant="ghost"
                  size="icon-sm"
                  onClick={() => setRevealed((r) => !r)}
                  aria-label={revealed ? "Hide API key" : "Reveal API key"}
                >
                  {revealed ? <EyeOff /> : <Eye />}
                </Button>
                {revealed && (
                  <Button
                    variant="ghost"
                    size="icon-sm"
                    onClick={() => void navigator.clipboard?.writeText(game.api_key ?? "")}
                    aria-label="Copy API key"
                  >
                    <Copy />
                  </Button>
                )}
              </div>
            )}
          </div>
        </div>
      </Panel>

      <Panel
        title="Configuration"
        description="Tick rate, capacity, and persistence parameters."
        actions={
          onConfigure !== undefined ? (
            <Button size="sm" variant="outline" onClick={onConfigure}>
              <Settings2 />
              Configure
            </Button>
          ) : undefined
        }
      >
        <div className="grid grid-cols-2 gap-x-6 px-4 py-3">
          <ConfigFields config={game.config} />
          <Field label="Status">{game.status}</Field>
          <Field label="Created">{formatTime(game.created_at)}</Field>
          <Field label="Updated">{formatTime(game.updated_at)}</Field>
        </div>
      </Panel>

      <Panel title="Connection details" description="Endpoints and settings for client SDKs.">
        <div className="flex flex-wrap gap-2 px-4 py-3">
          <Button size="sm" onClick={copyConnectionJson} disabled={copyingJson}>
            {copyingJson ? <Loader2 className="animate-spin" /> : <Braces />}
            Copy connection JSON
          </Button>
          <Button size="sm" variant="outline" onClick={downloadAsset} disabled={downloading}>
            {downloading ? <Loader2 className="animate-spin" /> : <Download />}
            Download settings asset
          </Button>
        </div>
      </Panel>
    </div>
  )
}

// ------------------------------------------------------------- shared: rooms

/** The Live Rooms tab is the matches/room feed the Matches area already reads —
 * `GET /v2/console/match` — reused here rather than re-implemented. A selected
 * room opens the same `MatchDetail` the Matches drawer renders. */
function LiveRoomsTab() {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const list = useConsoleQuery<ConsoleMatchList>("matches", CONSOLE_ENDPOINTS.matches, { limit: 50, offset: 0 })
  const rows = list.data?.matches
  const selected = rows?.find((match) => match.api_match.match_id === selectedId) ?? null

  return (
    <div className="space-y-4">
      <Panel
        title="Live rooms"
        description={list.data === undefined ? "Loading…" : `${rows?.length ?? 0} running · this node`}
      >
        <div className="divide-y divide-border/40">
          {list.isPending && <p className="px-4 py-3 text-sm text-muted-foreground">Loading rooms…</p>}
          {list.error !== null && list.error !== undefined && (
            <p className="px-4 py-3 text-sm text-destructive">{queryError(list.error)}</p>
          )}
          {!list.isPending && list.error == null && (rows?.length ?? 0) === 0 && (
            <p className="px-4 py-3 text-sm text-muted-foreground">No rooms running on this node.</p>
          )}
          {rows?.map((match) => (
            <RoomRow
              key={match.api_match.match_id}
              match={match}
              active={match.api_match.match_id === selectedId}
              onSelect={() => setSelectedId(match.api_match.match_id)}
            />
          ))}
        </div>
      </Panel>

      {selected !== null && (
        <MatchDetail key={selected.api_match.match_id} match={selected} onDeleted={() => setSelectedId(null)} />
      )}
    </div>
  )
}

function RoomRow({ match, active, onSelect }: { match: ConsoleMatch; active: boolean; onSelect: () => void }) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={`flex w-full items-center justify-between gap-3 px-4 py-2 text-left text-xs transition-colors hover:bg-muted/40 ${
        active ? "bg-primary/10" : ""
      }`}
    >
      <span className="min-w-0 font-medium text-foreground">{match.api_match.match_id}</span>
      <span className="flex shrink-0 items-center gap-2 text-muted-foreground">
        <Mono>{num(match.api_match.size)}p</Mono>
        {match.api_match.authoritative ? (
          <Badge variant="default">Authoritative</Badge>
        ) : (
          <Badge variant="outline">Relay</Badge>
        )}
      </span>
    </button>
  )
}

// ---------------------------------------------------------- wasm studio tabs

function WasmStudioTab({ game }: { game: GameDetail }) {
  return (
    <div className="space-y-4">
      <p className="text-xs text-muted-foreground">
        This game is server-authoritative ({engineLabel(game.engine_type)}); its room logic runs in
        WebAssembly. Upload a module, verify its ABI, then activate or roll it back below.
      </p>
      <WasmUploadPanel gameId={game.id} />
    </div>
  )
}

function MapsTab({ game }: { game: GameDetail }) {
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <MapsUploadPanel gameId={game.id} />
      <MapsPanel gameId={game.id} />
    </div>
  )
}

// ------------------------------------------------------------- configure form

const TICK_OPTIONS = TICK_RATE_OPTIONS.map((hz) => ({ value: String(hz), label: `${hz} Hz` }))
const CAPACITY_OPTIONS_LIST = CAPACITY_OPTIONS.map((n) => ({ value: String(n), label: `${n} players` }))

function ConfigureGameSheet({ game, onClose }: { game: GameDetail; onClose: () => void }) {
  const [name, setName] = useState(game.name)
  const [status, setStatus] = useState(game.status)
  const [tickRate, setTickRate] = useState(String(game.config.tick_rate))
  const [maxClients, setMaxClients] = useState(String(game.config.max_clients))
  const [persistentRooms, setPersistentRooms] = useState(game.config.persistent_rooms)
  const [autoClose, setAutoClose] = useState(String(game.config.auto_close_seconds))
  const write = useConsoleMutation()

  function confirm() {
    if (name.trim() === "") return
    write.mutate(
      {
        method: "PUT",
        path: gamePath(game.id),
        body: {
          name: name.trim(),
          engine_type: "custom",
          status,
          tick_rate: Number(tickRate) || 0,
          max_clients: Number(maxClients) || 0,
          persistent_rooms: persistentRooms,
          auto_close_seconds: Number(autoClose) || 0,
        },
        success: "Game updated.",
      },
      { onSuccess: onClose },
    )
  }

  return (
    <FormSheet
      open
      title="Configure game"
      description={game.game_key}
      onClose={onClose}
      footer={
        <Button onClick={confirm} disabled={write.isPending || name.trim() === ""}>
          {write.isPending && <Loader2 className="animate-spin" />}
          Save changes
        </Button>
      }
    >
      <TextInput label="Name" value={name} onChange={setName} />
      <Select label="Status" value={status} onChange={setStatus} options={STATUS_OPTIONS} />
      <div className="grid grid-cols-2 gap-3">
        <Select label="Sync rate" value={tickRate} onChange={setTickRate} options={TICK_OPTIONS} />
        <Select label="Max players" value={maxClients} onChange={setMaxClients} options={CAPACITY_OPTIONS_LIST} />
      </div>
      <div className="space-y-1 rounded-md border border-border/60 p-2.5">
        <Checkbox label="Persistent rooms" checked={persistentRooms} onChange={setPersistentRooms} />
        <p className="text-[11px] text-muted-foreground">
          Keep a room alive between matches so player-built bases and world state survive.
        </p>
      </div>
      {persistentRooms && (
        <TextInput
          label="Auto-close after (seconds)"
          hint="How long an empty persistent room stays open before the server closes it."
          type="number"
          value={autoClose}
          onChange={setAutoClose}
        />
      )}
    </FormSheet>
  )
}

// ----------------------------------------------------------------- the detail

export function GameDetailView({
  game,
  onArchived,
}: {
  game: GameDetail
  onArchived: () => void
}) {
  const [configuring, setConfiguring] = useState(false)
  const [archiving, setArchiving] = useState(false)

  return (
    <div className="space-y-4">
      <Panel
        title={game.name}
        description={game.game_key}
        actions={
          <div className="flex gap-1.5">
            <Button size="sm" variant="outline" onClick={() => setConfiguring(true)}>
              <Settings2 />
              Configure
            </Button>
            <Button size="sm" variant="destructive" onClick={() => setArchiving(true)}>
              <Trash2 />
              Archive
            </Button>
          </div>
        }
      >
        <div className="flex flex-wrap items-center gap-2 px-4 py-3">
          <StatusPill status={game.status} />
        </div>
        <div className="grid grid-cols-2 gap-x-6 px-4 pb-3">
          <ConfigFields config={game.config} />
        </div>
      </Panel>

      <Tabs defaultValue="overview">
        <TabsList className="w-full justify-start">
          <TabsTrigger value="overview">Configuration</TabsTrigger>
          <TabsTrigger value="maps">Maps</TabsTrigger>
          <TabsTrigger value="modules">WASM Modules</TabsTrigger>
          <TabsTrigger value="rooms">Live Rooms</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="pt-3">
          <CredentialsTab game={game} onConfigure={() => setConfiguring(true)} />
        </TabsContent>

        <TabsContent value="maps" className="pt-3">
          <MapsTab game={game} />
        </TabsContent>

        <TabsContent value="modules" className="pt-3">
          <div className="space-y-4">
            <WasmStudioTab game={game} />
            <DeploymentHistory gameId={game.id} />
          </div>
        </TabsContent>

        <TabsContent value="rooms" className="pt-3">
          <LiveRoomsTab />
        </TabsContent>
      </Tabs>

      {configuring && <ConfigureGameSheet game={game} onClose={() => setConfiguring(false)} />}

      <ConfirmAction
        open={archiving}
        onClose={() => setArchiving(false)}
        onDone={onArchived}
        title="Archive game"
        message="The game stops accepting clients and leaves the active catalog; its data and modules are kept. This is how a game retires."
        confirmLabel="Archive game"
        destructive
        method="DELETE"
        path={gamePath(game.id)}
        success="Game archived."
      />
    </div>
  )
}
