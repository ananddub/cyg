// The console chrome: the UI-kit sidebar (context header, searchable collapsible
// area tree, operator card footer) shared by every area. The inset variant is
// what curves the content into its own rounded card.
//
// The header and the tree follow the scope: at the landing it is the brand and
// the cross-project areas; inside a game it is a back button, the game's
// identity, and only that game's areas.

import { ChevronLeft, Gamepad2, LogOut, UserRound } from "lucide-react"

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
          <div className="space-y-2">
            {onBack !== undefined && (
              // The whole row is the back control, the way the reference draws
              // its context header: a chevron cell plus a centered title.
              <button
                type="button"
                onClick={onBack}
                title="Back to all games"
                aria-label="Back to all games"
                className="flex h-8 w-full items-center rounded-md text-sm font-semibold text-sidebar-foreground transition-colors hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-ring/40 focus-visible:outline-none group-data-[collapsible=icon]:justify-center"
              >
                <span className="flex size-7 shrink-0 items-center justify-center">
                  <ChevronLeft className="size-4 text-muted-foreground" strokeWidth={2} />
                </span>
                <span className="min-w-0 flex-1 truncate pr-7 text-center group-data-[collapsible=icon]:hidden">
                  All games
                </span>
              </button>
            )}
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
        <ConsoleNav area={area} areas={areas} onNavigate={onNavigate} flat={flatNav} />
      </SidebarContent>

      <SidebarFooter className="overflow-hidden">
        <div className="rounded-xl border border-sidebar-border bg-sidebar-accent p-4 shadow-sm group-data-[collapsible=icon]:hidden">
          <p className="text-sm font-semibold text-sidebar-foreground">{session.username}</p>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {session.email !== undefined && session.email !== ""
              ? session.email
              : "Signed-in operator"}
          </p>
          <div className="mt-2 flex items-center justify-between gap-2">
            <Badge variant="outline" className="border-border text-[10px]">
              {session.role}
            </Badge>
            <Button
              variant="default"
              size="sm"
              className="h-7 gap-1.5 rounded-lg px-3 text-xs"
              onClick={onLogout}
            >
              <LogOut />
              Sign out
            </Button>
          </div>
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
