// The console's area catalogue: the sidebar, the hash router and the page switch
// all read this one list, so a new area is a single entry plus its view.
//
// The catalogue is split by scope. A game workspace answers project-level areas
// (metrics, players, game data); the landing screen answers the cross-project
// ones (the games picker and console administration). The sidebar is built from
// whichever scope is active, which is why selecting a game changes the nav.

import {
  Activity,
  Bell,
  CreditCard,
  Cpu,
  Database,
  Fingerprint,
  Gamepad2,
  KeyRound,
  Layers,
  LayoutDashboard,
  Medal,
  RefreshCw,
  ScrollText,
  Settings,
  ShieldAlert,
  ShieldCheck,
  Swords,
  Trophy,
  Upload,
  UserCog,
  Users,
  UsersRound,
} from "lucide-react"

export interface ConsoleArea {
  group: string
  id: string
  label: string
  icon: typeof Activity
}

/** The landing area: the games picker. */
export const GAMES_AREA_ID = "games"
/** Where entering a game lands: its live metrics. */
export const GAME_DEFAULT_AREA = "runtime"

export const CONSOLE_AREAS: ConsoleArea[] = [
  // Cross-project (landing) areas.
  { group: "Workspace", id: GAMES_AREA_ID, label: "Games & Projects", icon: Gamepad2 },

  // Inside one game.
  { group: "Project", id: "project", label: "Project", icon: LayoutDashboard },
  { group: "Monitoring", id: "runtime", label: "Overview & Realtime", icon: Activity },
  { group: "Monitoring", id: "rooms", label: "Game Rooms", icon: Layers },
  { group: "Monitoring", id: "clients", label: "Connected Clients", icon: Users },
  { group: "Monitoring", id: "matches", label: "Matches & Matchmaking", icon: Swords },
  { group: "Monitoring", id: "cores", label: "Simulation Cores", icon: Cpu },
  { group: "Monitoring", id: "limits", label: "Limits & Guardrails", icon: ShieldAlert },
  { group: "Players", id: "accounts", label: "Accounts", icon: UserCog },
  { group: "Players", id: "groups", label: "Groups", icon: UsersRound },
  { group: "Players", id: "notifications", label: "Notifications", icon: Bell },
  { group: "Game Data", id: "wasm", label: "WASM Studio", icon: Upload },
  { group: "Game Data", id: "leaderboards", label: "Leaderboards", icon: Trophy },
  { group: "Game Data", id: "tournaments", label: "Tournaments", icon: Medal },
  { group: "Game Data", id: "storage", label: "Storage", icon: Database },
  { group: "Monetization", id: "purchases", label: "Purchases", icon: CreditCard },
  { group: "Monetization", id: "subscriptions", label: "Subscriptions", icon: RefreshCw },

  // Console administration stays at the landing scope: it is about operators,
  // not about one project's players or metrics.
  { group: "Administration", id: "users", label: "Console Users", icon: ShieldCheck },
  { group: "Administration", id: "audit", label: "Audit Log", icon: ScrollText },
  { group: "Administration", id: "acl", label: "ACL Templates", icon: KeyRound },
  { group: "Administration", id: "settings", label: "Settings", icon: Settings },
  { group: "Administration", id: "security", label: "My Security", icon: Fingerprint },
]

const GLOBAL_AREA_IDS = new Set([
  GAMES_AREA_ID,
  "users",
  "audit",
  "acl",
  "settings",
  "security",
])

const GAME_AREA_IDS = new Set([
  "project",
  "runtime",
  "rooms",
  "clients",
  "matches",
  "cores",
  "limits",
  "accounts",
  "groups",
  "notifications",
  "wasm",
  "leaderboards",
  "tournaments",
  "storage",
  "purchases",
  "subscriptions",
])

/** The areas the landing (no game selected) sidebar offers. */
export function globalAreas(): ConsoleArea[] {
  return CONSOLE_AREAS.filter((area) => GLOBAL_AREA_IDS.has(area.id))
}

/** The areas one game's workspace offers. */
export function gameAreas(): ConsoleArea[] {
  return CONSOLE_AREAS.filter((area) => GAME_AREA_IDS.has(area.id))
}

export function isGlobalArea(id: string): boolean {
  return GLOBAL_AREA_IDS.has(id)
}

export function isGameArea(id: string): boolean {
  return GAME_AREA_IDS.has(id)
}

/** The areas in order, grouped for the sidebar's section headings. */
export function groupedAreas(areas: ConsoleArea[]): { group: string; areas: ConsoleArea[] }[] {
  const groups: { group: string; areas: ConsoleArea[] }[] = []
  for (const area of areas) {
    const last = groups.at(-1)
    if (last !== undefined && last.group === area.group) last.areas.push(area)
    else groups.push({ group: area.group, areas: [area] })
  }
  return groups
}

export function findArea(id: string): ConsoleArea | undefined {
  return CONSOLE_AREAS.find((area) => area.id === id)
}

/** The landing fallback, never undefined: it is what an unknown hash shows. */
export function defaultGlobalArea(): ConsoleArea {
  return findArea(GAMES_AREA_ID) ?? CONSOLE_AREAS[0]
}

/** The game workspace's fallback, never undefined. */
export function defaultGameArea(): ConsoleArea {
  return findArea(GAME_DEFAULT_AREA) ?? (gameAreas()[0] ?? CONSOLE_AREAS[0])
}
