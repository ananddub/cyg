// The sidebar's navigation tree: a search field and one collapsible menu per
// area group. The search filters the catalogue and forces every group with a
// match open; without a query only the group holding the active area is open,
// which is what keeps the tree scannable at nineteen entries.

import { useEffect, useMemo, useRef, useState } from "react"
import {
  Activity,
  ChevronRight,
  CreditCard,
  Database,
  Search,
  ShieldCheck,
  UsersRound,
  X,
  type LucideIcon,
} from "lucide-react"

import {
  SidebarGroup,
  SidebarGroupLabel,
  SidebarInput,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
} from "@/components/ui/sidebar"
import { groupedAreas, type ConsoleArea } from "@/lib/areas"
import { cn } from "@/lib/utils"

const GROUP_ICONS: Record<string, LucideIcon> = {
  Monitoring: Activity,
  Players: UsersRound,
  "Game Data": Database,
  Monetization: CreditCard,
  Administration: ShieldCheck,
}

export function ConsoleNav({
  area,
  onNavigate,
}: {
  area: ConsoleArea
  onNavigate: (id: string) => void
}) {
  const [query, setQuery] = useState("")
  const [open, setOpen] = useState<Record<string, boolean>>({ [area.group]: true })
  const input = useRef<HTMLInputElement>(null)
  const onMac = useMemo(
    () => /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent),
    [],
  )

  // ⌘K (or Ctrl+K) focuses the filter, the same gesture the field's badge shows.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault()
        input.current?.focus()
        input.current?.select()
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [])

  const searching = query.trim() !== ""
  const groups = groupedAreas().map((group) => ({
    ...group,
    areas: searching
      ? group.areas.filter((candidate) =>
          candidate.label.toLowerCase().includes(query.trim().toLowerCase()),
        )
      : group.areas,
  }))

  return (
    <div className="flex flex-col gap-1">
      <SidebarGroup className="pb-0">
        <div className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 z-10 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <SidebarInput
            ref={input}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search..."
            className="pl-8 pr-14"
          />
          {searching ? (
            <button
              type="button"
              aria-label="Clear search"
              onClick={() => setQuery("")}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
            >
              <X className="size-3.5" />
            </button>
          ) : (
            <kbd className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 rounded border border-border bg-background px-1.5 font-mono text-[10px] text-muted-foreground">
              {onMac ? "⌘K" : "Ctrl K"}
            </kbd>
          )}
        </div>
      </SidebarGroup>

      <SidebarGroup>
        <SidebarGroupLabel>Console</SidebarGroupLabel>
        <SidebarMenu>
          {groups.map((group) => {
            const GroupIcon = GROUP_ICONS[group.group] ?? UsersRound
            const expanded = searching ? group.areas.length > 0 : (open[group.group] ?? false)
            if (searching && group.areas.length === 0) return null
            return (
              <SidebarMenuItem key={group.group}>
                <SidebarMenuButton
                  onClick={() =>
                    setOpen((current) => ({ ...current, [group.group]: !expanded }))
                  }
                  title={group.group}
                  className="font-medium text-sidebar-foreground/85"
                >
                  <GroupIcon />
                  <span>{group.group}</span>
                  <ChevronRight
                    className={cn(
                      "ml-auto size-4 transition-transform",
                      expanded && "rotate-90",
                    )}
                  />
                </SidebarMenuButton>
                {expanded && (
                  <SidebarMenuSub>
                    {group.areas.map((candidate) => (
                      <SidebarMenuSubItem key={candidate.id}>
                        <SidebarMenuSubButton
                          isActive={candidate.id === area.id}
                          onClick={() => onNavigate(candidate.id)}
                          className="font-medium text-sidebar-foreground/85"
                        >
                          <span>{candidate.label}</span>
                        </SidebarMenuSubButton>
                      </SidebarMenuSubItem>
                    ))}
                  </SidebarMenuSub>
                )}
              </SidebarMenuItem>
            )
          })}
        </SidebarMenu>
      </SidebarGroup>
    </div>
  )
}
