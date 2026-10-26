// The Games Catalog's wire shapes. One home for them keeps the view, the register
// modal, the detail sheet and the module panels reading the same contract without
// a component->component import cycle.

import type { BackendMode } from "./routes"

/** `config` as of the revised plan: the old physics / lag_compensation /
 * allow_storage keys are gone — room lifecycle replaced them. */
export interface GameConfig {
  tick_rate: number
  max_clients: number
  persistent_rooms: boolean
  auto_close_seconds: number
}

export interface Game {
  id: string
  game_key: string
  name: string
  client_key: string
  engine_type: string
  backend_mode: BackendMode
  status: string
  config: GameConfig
  created_at: string
  updated_at: string
}

/** The list envelope. */
export interface GameList {
  games: Game[]
  total: number
}

/** `GET /v2/console/games/{id}`: the game plus what only the detail carries. */
export interface GameDetail extends Game {
  api_key?: string
  active_module_version?: number | null
  module_count?: number
}

/** A creation response is a game whose `api_key` is guaranteed present, shown
 * once, and never returned again by the list. */
export interface CreatedGame extends Game {
  api_key: string
}

/** `GET /v2/console/games/{id}/unity-settings?format=json` — the same facts the
 * `.asset` carries, as a copyable document. */
export interface UnitySettingsJson {
  gameKey: string
  clientKey: string
  serverHttpUrl: string
  serverWsPath: string
  defaultRoom: string
  tickRate: number
  maxPlayers: number
  persistentRooms: boolean
  backendMode: string
}

export interface WasmModuleInfo {
  id: string
  version: number
  filename: string
  sha256: string
  is_active: boolean
  size_bytes: number
  has_source: boolean
  created_at: string
}

export interface AbiReport {
  valid: boolean
  wasm_magic_ok: boolean
  exports_found: string[]
  exports_missing: string[]
  export_count: number
  size_bytes: number
}
