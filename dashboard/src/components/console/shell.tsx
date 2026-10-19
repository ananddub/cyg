// The console chrome: the UI-kit sidebar (name header, searchable collapsible
// area tree, operator card footer) shared by every area. The inset variant is
// what curves the content into its own rounded card.

import { Gamepad2, LogOut } from "lucide-react"

import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
} from "@/components/ui/sidebar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { ConsoleArea } from "@/lib/areas"
import type { ConsoleSession } from "@/lib/console"

import { ConsoleNav } from "./console-nav"

export function ConsoleSidebar({
  area,
  onNavigate,
  session,
  onLogout,
}: {
  area: ConsoleArea
  onNavigate: (id: string) => void
  session: ConsoleSession
  onLogout: () => void
}) {
  return (
    <Sidebar variant="inset" collapsible="offcanvas">
      <SidebarHeader className="py-4">
        <div className="flex items-center gap-2.5 px-1">
          <span className="flex size-8 items-center justify-center rounded-lg bg-sidebar-accent text-sidebar-foreground">
            <Gamepad2 className="size-4.5" />
          </span>
          <span className="text-sm font-semibold tracking-tight text-sidebar-foreground">
            cookyourgame
          </span>
        </div>
      </SidebarHeader>

      <SidebarContent>
        <ConsoleNav area={area} onNavigate={onNavigate} />
      </SidebarContent>

      <SidebarFooter>
        <div className="rounded-xl border border-sidebar-border bg-sidebar-accent p-4 shadow-sm">
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
      </SidebarFooter>
    </Sidebar>
  )
}
