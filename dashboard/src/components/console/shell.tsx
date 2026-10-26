// The console chrome: the UI-kit sidebar (context header, searchable collapsible
// area tree, operator footer) shared by every area. The inset variant is
// what curves the content into its own rounded card.
//
// The header and the tree follow the scope: at the landing it is the brand and
// the cross-project areas; inside a game it is the game's identity, and only
// that game's areas under a back row that sits below the filter.

import { Gamepad2, LogOut, UserRound } from "lucide-react"

import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  useSidebar,
} from "@/components/ui/sidebar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { ConsoleArea } from "@/lib/areas"
import type { ConsoleSession } from "@/lib/console"

import type { Game } from "./views/games"
import { ConsoleNav } from "./console-nav"

export function ConsoleSidebar({
  area,
  areas,
  onNavigate,
  session,
  onLogout,
  game = null,
  onBack,
  flatNav = false,
}: {
  area: ConsoleArea
  areas: ConsoleArea[]
  onNavigate: (id: string) => void
  session: ConsoleSession
  onLogout: () => void
  /** The game the workspace is scoped to, or `null` at the landing. */
  game?: Game | null
  /** Present only inside a game: returns to the games picker. */
  onBack?: () => void
  /** The landing lists its few areas without group headings. */
  flatNav?: boolean
}) {
  const { toggleSidebar } = useSidebar()

  return (
    // The rail collapses to icons rather than sliding off-canvas: the surface
    // stays put, so opening/closing reads as a rail narrowing, never the
    // sidebar dissolving into the near-black page.
    <Sidebar variant="sidebar" collapsible="icon">
      <SidebarHeader className="overflow-hidden py-4">
        {game !== null ? (
          <div
            className="flex items-center gap-2.5 px-1 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0"
            title={game.name}
          >
            <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-sidebar-accent text-sidebar-foreground">
              <Gamepad2 className="size-4.5" />
            </span>
            <div className="min-w-0 flex-1 group-data-[collapsible=icon]:hidden">
              <p className="truncate text-sm font-semibold tracking-tight text-sidebar-foreground">
                {game.name}
              </p>
              <p className="truncate font-mono text-[10px] text-muted-foreground">
                {game.game_key}
              </p>
            </div>
            <Badge
              variant="outline"
              className="shrink-0 border-border text-[10px] group-data-[collapsible=icon]:hidden"
            >
              {game.status}
            </Badge>
          </div>
        ) : (
          <div
            className="flex items-center gap-2.5 px-1 group-data-[collapsible=icon]:justify-center group-data-[collapsible=icon]:px-0"
            title="cookyourgame"
          >
            <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-sidebar-accent text-sidebar-foreground">
              <Gamepad2 className="size-4.5" />
            </span>
            <span className="truncate text-sm font-semibold tracking-tight text-sidebar-foreground group-data-[collapsible=icon]:hidden">
              cookyourgame
            </span>
          </div>
        )}
      </SidebarHeader>

      <SidebarContent>
        <ConsoleNav
          area={area}
          areas={areas}
          onNavigate={onNavigate}
          flat={flatNav}
          onBack={onBack}
        />
      </SidebarContent>

      <SidebarFooter className="overflow-hidden">
        <div className="flex items-center gap-2.5 px-1 py-1.5 group-data-[collapsible=icon]:hidden">
          <span className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-sidebar-accent text-sidebar-foreground">
            <UserRound className="size-4" />
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-1.5">
              <p className="truncate text-sm font-medium text-sidebar-foreground">
                {session.username}
              </p>
              <Badge variant="outline" className="shrink-0 border-border px-1.5 text-[10px]">
                {session.role}
              </Badge>
            </div>
            <p className="truncate text-[11px] text-muted-foreground">
              {session.email !== undefined && session.email !== ""
                ? session.email
                : "Signed-in operator"}
            </p>
          </div>
          <Button
            variant="ghost"
            size="icon-sm"
            className="shrink-0 text-muted-foreground hover:text-foreground"
            onClick={onLogout}
            title="Sign out"
          >
            <LogOut />
            <span className="sr-only">Sign out</span>
          </Button>
        </div>
        {/* Icon rail: the account mark reopens the rail, where Sign out lives. */}
        <SidebarMenu className="hidden group-data-[collapsible=icon]:block">
          <SidebarMenuItem>
            <SidebarMenuButton tooltip="Account" onClick={toggleSidebar}>
              <UserRound />
              <span className="sr-only">Account</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>

      <SidebarRail />
    </Sidebar>
  )
}
