// The Games Catalog area's door: the view from `index.tsx` and the route helpers
// from `routes.ts`, re-exported the way `administration/index.ts` exports its
// area modules.

export { GamesView } from "./index.tsx"
export {
  GAMES_ENDPOINT,
  RELAY_ENGINES,
  RELAY_WASM_REJECTED,
  WASM_ENGINES,
  gamePath,
  gameUnitySettingsPath,
  gameMapsPath,
  gameWasmPath,
  isRelayMode,
  isWasmMode,
} from "./routes"
export type { BackendMode } from "./routes"
export { MapsPanel, MapsUploadPanel } from "./maps"
export type { GameMapInfo } from "./maps"
export type {
  AbiReport,
  CreatedGame,
  Game,
  GameConfig,
  GameDetail,
  GameList,
  UnitySettingsJson,
  WasmModuleInfo,
} from "./index.tsx"
