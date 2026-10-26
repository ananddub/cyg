// The games area's non-component exports: the routes, the per-game paths, the
// engine/backend-mode contract and the one-value hand-off to the WASM Studio.
// They live apart from the view so the view file exports components only — the
// studio imports these without pulling in a render.

import { gameRoute, routeHash } from "@/lib/routes"

export const GAMES_ENDPOINT = "/v2/console/games"

export function gamePath(id: string): string {
  return `${GAMES_ENDPOINT}/${encodeURIComponent(id)}`
}

/** The module list route for a game; the studio and its timeline share it. */
export function gameWasmPath(id: string): string {
  return `${gamePath(id)}/wasm`
}

/** `GET /v2/console/games/{id}/unity-settings` — the Unity `CYGServerSettings.asset`.
 * `?format=asset` is a Content-Disposition download; `?format=json` is the same
 * data as a document the Credentials tab can copy. */
export function gameUnitySettingsPath(id: string): string {
  return `${gamePath(id)}/unity-settings`
}

// ------------------------------------------------------------- backend modes
//
// `backend_mode` is owned by the server: registration sends an `engine_type` and
// the answer comes back already tagged `relay` or `wasm`. Nothing here derives the
// mode from the engine for a request — these constants only describe the contract
// so the UI can group, label and gate on the field the server returned.

export type BackendMode = "relay" | "wasm"

/** Engines the server maps to `backend_mode = "relay"` (Zero Server Code — Pure
 * C# Relay & BaaS); for these the WASM plane is invisible. */
export const RELAY_ENGINES = ["unity", "unreal", "godot"] as const

/** Engines the server maps to `backend_mode = "wasm"` (server-side simulation). */
export const WASM_ENGINES = ["web", "custom"] as const

/** The message the WASM routes return (HTTP 400) when they are reached with a
 * relay game — a stale deep link. Rendered as an explanatory empty state. */
export const RELAY_WASM_REJECTED =
  "Game is configured for Unity C# Relay. WASM upload is only applicable for Web/Custom Authoritative games."

export function isRelayMode(mode: BackendMode | string | undefined): boolean {
  return mode === "relay"
}

export function isWasmMode(mode: BackendMode | string | undefined): boolean {
  return mode === "wasm"
}

// The Studio is an area of the game workspace, not a separate global page: the
// hand-off navigates to that game's WASM tab, so leaving it keeps the game in
// scope and the back button still points at the picker.
export function openWasmStudio(gameId: string): void {
  window.location.hash = routeHash(gameRoute(gameId, "wasm"))
}
