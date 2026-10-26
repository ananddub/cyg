// The sidebar's navigation tree: a search field and one collapsible menu per
// area group. The search filters the catalogue and forces every group with a
// match open; without a query only the group holding the active area is open,
// which is what keeps the tree scannable at nineteen entries.
//
// The landing has only a handful of areas, so it asks for `flat`: one row per
// area, no group headings and nothing to expand.

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  Activity,
  ChevronRight,
  CreditCard,
  Database,
  Gamepad2,
  LayoutDashboard,
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
  useSidebar,
} from "@/components/ui/sidebar"
import { groupedAreas, type ConsoleArea } from "@/lib/areas"
import { cn } from "@/lib/utils"

const GROUP_ICONS: Record<string, LucideIcon> = {
  Workspace: Gamepad2,
  Project: LayoutDashboard,
  Monitoring: Activity,
  Players: UsersRound,
  "Game Data": Database,
  Monetization: CreditCard,
  Administration: ShieldCheck,
}

export function ConsoleNav({
  area,
  areas,
  onNavigate,
  flat = false,
}: {
  area: ConsoleArea
  areas: ConsoleArea[]
  onNavigate: (id: string) => void
  /** Render every area as a plain row instead of collapsible groups. */
  flat?: boolean
}) {
  const [query, setQuery] = useState("")
  const [open, setOpen] = useState<Record<string, boolean>>({ [area.group]: true })
  const input = useRef<HTMLInputElement>(null)
  const wantFocus = useRef(false)
  const { state, isMobile, toggleSidebar } = useSidebar()
  const collapsed = state === "collapsed" && !isMobile
  const onMac = useMemo(
    () => /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent),
    [],
  )

  // ⌘K (or Ctrl+K) focuses the filter, the same gesture the field's badge shows.
  const focusSearch = useCallback(() => {
    if (collapsed) {
      // The field does not exist in the icon rail; reopen and focus it once
      // the width transition has settled.
      wantFocus.current = true
      toggleSidebar()
      return
    }
    input.current?.focus()
    input.current?.select()
  }, [collapsed, toggleSidebar])

  useEffect(() => {
    if (collapsed || !wantFocus.current) return
    wantFocus.current = false
    const timer = window.setTimeout(() => {
      input.current?.focus()
      input.current?.select()
    }, 200)
    return () => window.clearTimeout(timer)
  }, [collapsed])

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault()
        focusSearch()
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [focusSearch])

  const searching = query.trim() !== ""
  const matches = (candidate: ConsoleArea) =>
    candidate.label.toLowerCase().includes(query.trim().toLowerCase())
  const visible = searching ? areas.filter(matches) : areas
  const groups = groupedAreas(areas).map((group) => ({
    ...group,
    areas: searching ? group.areas.filter(matches) : group.areas,
  }))

  return (
    <div className="flex flex-col gap-1">
      {collapsed ? (
        // Icon rail: the field folds into a single magnifier row that reopens
        // the sidebar and lands the caret in the search input.
        <SidebarGroup className="pb-0">
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton tooltip="Search" onClick={focusSearch}>
                <Search />
                <span>Search</span>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarGroup>
      ) : (
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
      )}

      <SidebarGroup>
        {!flat && <SidebarGroupLabel>Console</SidebarGroupLabel>}
        <SidebarMenu>
          {flat ? (
            visible.map((candidate) => (
              <SidebarMenuItem key={candidate.id}>
                <SidebarMenuButton
                  isActive={candidate.id === area.id}
                  onClick={() => onNavigate(candidate.id)}
                  tooltip={candidate.label}
                  className="font-medium text-sidebar-foreground/85"
                >
                  <candidate.icon />
                  <span>{candidate.label}</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            ))
          ) : groups.map((group) => {
            const GroupIcon = GROUP_ICONS[group.group] ?? UsersRound
            if (searching && group.areas.length === 0) return null
            // A group of one is that area: a collapsible header over a lone
            // child costs a click and tells the operator nothing extra.
            if (group.areas.length === 1) {
              const only = group.areas[0]
              return (
                <SidebarMenuItem key={group.group}>
                  <SidebarMenuButton
                    isActive={only.id === area.id}
                    onClick={() => onNavigate(only.id)}
                    tooltip={only.label}
                    className="font-medium text-sidebar-foreground/85"
                  >
                    <GroupIcon />
                    <span>{only.label}</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              )
            }
            const expanded = searching ? group.areas.length > 0 : (open[group.group] ?? false)
            return (
              <SidebarMenuItem key={group.group}>
                <SidebarMenuButton
                  isActive={group.areas.some((candidate) => candidate.id === area.id)}
                  onClick={() => {
                    // On the icon rail a group row cannot unfold in place, so
                    // it reopens the rail with that group expanded.
                    if (collapsed) {
                      setOpen((current) => ({ ...current, [group.group]: true }))
                      toggleSidebar()
                      return
                    }
                    setOpen((current) => ({ ...current, [group.group]: !expanded }))
                  }}
                  tooltip={group.group}
                  title={group.group}
                  className="font-medium text-sidebar-foreground/85"
                >
                  <GroupIcon />
                  <span>{group.group}</span>
                  <ChevronRight
                    className={cn(
                      "ml-auto size-4 transition-transform group-data-[collapsible=icon]:hidden",
                      expanded && "rotate-90",
                    )}
                  />
                </SidebarMenuButton>
                {expanded && (
                  <SidebarMenuSub className="motion-safe:animate-in motion-safe:fade-in-0 motion-safe:slide-in-from-top-1 motion-safe:duration-150">
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
