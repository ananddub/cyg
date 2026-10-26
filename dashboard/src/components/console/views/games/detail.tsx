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
  Check as CheckDone,
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
import { Field, JsonBlock, Mono, Panel } from "@/components/console/data-table"
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
import { CopyValue, EngineBadge, ModePill, StatusPill } from "./parts"

// ------------------------------------------------------------------ snippets
//
// Real, compiling C# — every call below is read off the shipped Unity SDK
// (sdk/unity/Runtime/Core/CYGNetwork.cs, CYGServerSettings.cs and the BaaS files
// CYGAuth / CYGStorage / CYGInventory / CYGLeaderboard). Method names are used
// exactly as the SDK declares them; nothing is invented.

const SNIPPET_DEVICE_LOGIN = `using CookYourGame;
using UnityEngine;

public class GameLauncher : MonoBehaviour
{
    async void Start()
    {
        // One-line guest login. Falls back to SystemInfo.deviceUniqueIdentifier
        // and persists the session token in PlayerPrefs for you.
        CYGAuthSession session = await CYGAuth.LoginDeviceAsync();
        if (session == null) return;                       // login failed
        Debug.Log($"signed in as {CYGAuth.Username} ({CYGAuth.UserId})");

        // Server endpoints come from the CYGServerSettings asset you download in
        // the Credentials tab; reserve a seat and open the room socket.
        CYGNetwork.ConnectToRoom("lobby", session.username);

        // ...or reserve-or-create a named room with a capacity hint:
        // CYGNetwork.JoinOrCreateRoom("arena-1", 8, session.username);
    }
}`

const SNIPPET_ROOM_CALLBACKS = `using CookYourGame;
using UnityEngine;

public class RoomHooks : MonoBehaviour
{
    void OnEnable()
    {
        CYGNetwork.OnConnectedEvent   += () => Debug.Log("room handshake done");
        CYGNetwork.OnJoinedRoom       += room  => Debug.Log($"joined {room}");
        CYGNetwork.OnDisconnectedEvent += why => Debug.Log($"left: {why}");
        CYGNetwork.OnErrorEvent       += (code, msg) => Debug.LogError($"{code}: {msg}");
        // Room is full arrives here as (4002, "room is full") just before OnDisconnectedEvent.
    }

    void OnDisable()
    {
        CYGNetwork.OnConnectedEvent   -= Noop;   // unsubscribe in a real build
        CYGNetwork.OnJoinedRoom       -= NoopRoom;
        CYGNetwork.OnDisconnectedEvent -= NoopWhy;
        CYGNetwork.OnErrorEvent       -= NoopErr;
    }

    void Update() => CYGNetwork.PumpInbound();   // drain the socket once per frame

    void Noop() {} void NoopRoom(string r) {} void NoopWhy(string w) {} void NoopErr(int c, string m) {}
}`

const SNIPPET_BAAS = `using System.Collections.Generic;
using CookYourGame;
using UnityEngine;

public class Loadout : MonoBehaviour
{
    async System.Threading.Tasks.Task Run()
    {
        // Save a document (game save / progression) — value is a JSON string.
        await CYGStorage.WriteAsync("saves", "slot1", "{\\"hp\\":100}");
        string json = await CYGStorage.ReadAsync("saves", "slot1");   // null when absent

        // Server-authoritative inventory.
        List<CYGInventoryItem> items = await CYGInventory.GetInventoryAsync();
        await CYGInventory.GrantItemAsync("sword_rusty", quantity: 1);
        await CYGInventory.EquipItemAsync(items[0].instance_id, slotIndex: 0);

        // Leaderboards.
        await CYGLeaderboard.SubmitScoreAsync("best_times", score: 4211);
        CYGLeaderboardRecordList top = await CYGLeaderboard.GetScoresAsync("best_times", limit: 10);
    }
}`

// ------------------------------------------------------------------ helpers

/** Reads the `filename=` out of a Content-Disposition header, quoted or bare. */
function filenameFromDisposition(header: string | null, fallback: string): string {
  if (header === null) return fallback
  const star = /filename\*\s*=\s*[^;]*'[^']*'?([^;']+)'?/i.exec(header)
  const plain = /filename\s*=\s*"?([^";]+)"?/i.exec(header)
  const raw = (star ?? plain)?.[1]?.trim()
  return raw === undefined || raw === "" ? fallback : decodeURIComponent(raw)
}

function CodeSnippet({ title, language, code }: { title: string; language: string; code: string }) {
  const [copied, setCopied] = useState(false)
  function copy() {
    navigator.clipboard
      ?.writeText(code)
      .then(() => {
        setCopied(true)
        window.setTimeout(() => setCopied(false), 1500)
      })
      .catch(() => setCopied(false))
  }
  return (
    <Panel
      title={title}
      actions={
        <Button size="xs" variant="outline" onClick={copy}>
          {copied ? <CheckDone className="size-3" /> : <Copy className="size-3" />}
          {copied ? "Copied" : `Copy ${language}`}
        </Button>
      }
    >
      <pre className="overflow-auto px-4 py-3 font-mono text-[11px] leading-relaxed text-foreground">
        <code>{code}</code>
      </pre>
    </Panel>
  )
}

// ------------------------------------------------------------- relay: SDK tab

function UnityQuickstartTab() {
  return (
    <div className="space-y-4">
      <p className="text-xs text-muted-foreground">
        Zero Server Code — you write no server logic. Authenticate, let the C# relay seat you into
        a room, and call the BaaS APIs. These snippets compile against the shipped
        <Mono className="mx-1">CookYourGame</Mono> SDK.
      </p>
      <CodeSnippet title="1 · Device login + join a room" language="C#" code={SNIPPET_DEVICE_LOGIN} />
      <CodeSnippet title="2 · Room callbacks and the per-frame pump" language="C#" code={SNIPPET_ROOM_CALLBACKS} />
      <CodeSnippet title="3 · BaaS: storage, inventory, leaderboards" language="C#" code={SNIPPET_BAAS} />
    </div>
  )
}

// ---------------------------------------------------- relay: credentials tab

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

function CredentialsTab({ game }: { game: GameDetail }) {
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
      <Panel title="Identifiers" description="Paste these into the Unity build's configuration.">
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

      <Panel title="Unity connection" description="Drives the CYGServerSettings asset for this game.">
        <div className="flex flex-wrap gap-2 px-4 py-3">
          <Button size="sm" onClick={copyConnectionJson} disabled={copyingJson}>
            {copyingJson ? <Loader2 className="animate-spin" /> : <Braces />}
            Copy connection JSON
          </Button>
          <Button size="sm" variant="outline" onClick={downloadAsset} disabled={downloading}>
            {downloading ? <Loader2 className="animate-spin" /> : <Download />}
            Download CYGServerSettings.asset
          </Button>
        </div>
        <p className="px-4 pb-3 text-xs text-muted-foreground">
          The SDK ships no <code>.meta</code> files, so a downloaded asset carries a script reference your project may
          not recognise yet. If it lands as <span className="font-medium">Missing Script</span> in the Inspector, drag
          <code> Assets/.../CYGServerSettings.cs</code> onto that field — the six values survive the re-bind. The
          connection JSON needs no re-bind and is the faster path for a fresh project.
        </p>
      </Panel>

      <Panel title="Configuration">
        <div className="px-4 py-2">
          <ConfigFields config={game.config} />
          <Field label="Engine">{engineLabel(game.engine_type)}</Field>
          <Field label="Status">{game.status}</Field>
          <Field label="Created">{formatTime(game.created_at)}</Field>
          <Field label="Updated">{formatTime(game.updated_at)}</Field>
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
          // engine_type is fixed at registration; the mode follows it, so keep
          // sending the current engine rather than letting it drift.
          engine_type: game.engine_type,
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
          Save
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
      <JsonBlock value={game.config} />
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

  const isWasm = game.backend_mode === "wasm"

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
          <EngineBadge engine={game.engine_type} mode={game.backend_mode} />
          <ModePill mode={game.backend_mode} />
          <StatusPill status={game.status} />
        </div>
        <div className="grid grid-cols-2 gap-x-6 px-4 pb-3">
          <ConfigFields config={game.config} />
          {isWasm && (
            <>
              <Field label="Modules">{num(game.module_count)}</Field>
              <Field label="Active version">
                {game.active_module_version === null || game.active_module_version === undefined
                  ? "—"
                  : `v${game.active_module_version}`}
              </Field>
            </>
          )}
        </div>
      </Panel>

      <Tabs defaultValue={isWasm ? "studio" : "sdk"}>
        <TabsList className="w-full justify-start">
          {isWasm ? (
            <>
              <TabsTrigger value="studio">WASM Studio</TabsTrigger>
              <TabsTrigger value="history">Deployment History</TabsTrigger>
              <TabsTrigger value="maps">Maps</TabsTrigger>
              <TabsTrigger value="rooms">Live Rooms</TabsTrigger>
            </>
          ) : (
            <>
              <TabsTrigger value="sdk">Unity SDK Quickstart</TabsTrigger>
              <TabsTrigger value="creds">Credentials &amp; Config</TabsTrigger>
              <TabsTrigger value="rooms">Live Rooms</TabsTrigger>
            </>
          )}
        </TabsList>

        {isWasm ? (
          <>
            <TabsContent value="studio" className="pt-3">
              <WasmStudioTab game={game} />
            </TabsContent>
            <TabsContent value="history" className="pt-3">
              <DeploymentHistory gameId={game.id} />
            </TabsContent>
            <TabsContent value="maps" className="pt-3">
              <MapsTab game={game} />
            </TabsContent>
          </>
        ) : (
          <>
            <TabsContent value="sdk" className="pt-3">
              <UnityQuickstartTab />
            </TabsContent>
            <TabsContent value="creds" className="pt-3">
              <CredentialsTab game={game} />
            </TabsContent>
          </>
        )}
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
