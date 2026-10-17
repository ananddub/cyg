// The console's area catalogue: the sidebar, the hash router and the page switch
// all read this one list, so a new area is a single entry plus its view.

import {
  Activity,
  Bell,
  CreditCard,
  Cpu,
  Database,
  Fingerprint,
  KeyRound,
  Layers,
  RefreshCw,
  ScrollText,
  Settings,
  ShieldAlert,
  ShieldCheck,
  Swords,
  Trophy,
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

export const CONSOLE_AREAS: ConsoleArea[] = [
  { group: "Monitoring", id: "runtime", label: "Overview & Realtime", icon: Activity },
  { group: "Monitoring", id: "rooms", label: "Game Rooms", icon: Layers },
  { group: "Monitoring", id: "clients", label: "Connected Clients", icon: Users },
  { group: "Monitoring", id: "matches", label: "Matches & Matchmaking", icon: Swords },
  { group: "Monitoring", id: "cores", label: "Simulation Cores", icon: Cpu },
  { group: "Monitoring", id: "limits", label: "Limits & Guardrails", icon: ShieldAlert },
  { group: "Players", id: "accounts", label: "Accounts", icon: UserCog },
  { group: "Players", id: "groups", label: "Groups", icon: UsersRound },
  { group: "Players", id: "notifications", label: "Notifications", icon: Bell },
  { group: "Game Data", id: "leaderboards", label: "Leaderboards", icon: Trophy },
  { group: "Game Data", id: "storage", label: "Storage", icon: Database },
  { group: "Monetization", id: "purchases", label: "Purchases", icon: CreditCard },
  { group: "Monetization", id: "subscriptions", label: "Subscriptions", icon: RefreshCw },
  { group: "Administration", id: "users", label: "Console Users", icon: ShieldCheck },
  { group: "Administration", id: "audit", label: "Audit Log", icon: ScrollText },
  { group: "Administration", id: "acl", label: "ACL Templates", icon: KeyRound },
  { group: "Administration", id: "settings", label: "Settings", icon: Settings },
  { group: "Administration", id: "security", label: "My Security", icon: Fingerprint },
]

export const DEFAULT_AREA = "runtime"

/** The areas in order, grouped for the sidebar's section headings. */
export function groupedAreas(): { group: string; areas: ConsoleArea[] }[] {
  const groups: { group: string; areas: ConsoleArea[] }[] = []
  for (const area of CONSOLE_AREAS) {
    const last = groups.at(-1)
    if (last !== undefined && last.group === area.group) last.areas.push(area)
    else groups.push({ group: area.group, areas: [area] })
  }
  return groups
}

export function findArea(id: string): ConsoleArea | undefined {
  return CONSOLE_AREAS.find((area) => area.id === id)
}

/** The fallback area, never undefined: the first entry is the default. */
export function defaultArea(): ConsoleArea {
  return findArea(DEFAULT_AREA) ?? CONSOLE_AREAS[0]
}

/** Whether `id` names a real area, so an unknown hash falls back rather than
 * rendering a blank page. */
export function isArea(id: string): boolean {
  return findArea(id) !== undefined
}
