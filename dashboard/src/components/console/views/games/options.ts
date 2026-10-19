// The registration/config option tables shared by the register modal, the
// configure sheet and the detail view. Constants live here (not in a component
// file) so Fast Refresh keeps working and both views read one source.

import type { BackendMode } from "./routes"

/** Room capacity options (max_clients / advertised players). */
export const CAPACITY_OPTIONS = [2, 4, 8, 16, 32, 64] as const

/** Network sync-rate options, in Hz. */
export const TICK_RATE_OPTIONS = [20, 30, 60, 128] as const

export interface EngineChoice {
  value: string
  label: string
  /** The mode the server is expected to tag this engine with — shown to the
   * developer only; the request sends just `engine_type`, never `backend_mode`. */
  mode: BackendMode
  badge: string
}

/** The five engines the catalog knows, grouped by the mode the server derives.
 * Unity is first so it is the default card. */
export const ENGINE_CHOICES: EngineChoice[] = [
  {
    value: "unity",
    label: "Unity",
    mode: "relay",
    badge: "Zero Server Code — Pure C# Relay & BaaS",
  },
  {
    value: "unreal",
    label: "Unreal Engine",
    mode: "relay",
    badge: "High-Performance Binary Relay",
  },
  {
    value: "godot",
    label: "Godot",
    mode: "relay",
    badge: "High-Performance Binary Relay",
  },
  {
    value: "web",
    label: "Web / Custom",
    mode: "wasm",
    badge: "Requires Server-Side WASM Simulation",
  },
]

export const STATUS_OPTIONS = [
  { value: "active", label: "Active" },
  { value: "suspended", label: "Suspended" },
  { value: "archived", label: "Archived" },
] as const

/** The one-line note shown under the Web/Custom card. */
export const WASM_PREREQ_NOTE =
  "Web / Custom games need an uploaded room module before players can join."

export function engineLabel(engine: string): string {
  return ENGINE_CHOICES.find((choice) => choice.value === engine)?.label ?? engine
}

export function modeBadge(mode: BackendMode | undefined, engine: string): string {
  if (mode === "wasm") return "Server-Side WASM Simulation"
  if (mode === "relay") {
    return ENGINE_CHOICES.find((choice) => choice.value === engine)?.badge ?? "Pure C# Relay & BaaS"
  }
  return ""
}
