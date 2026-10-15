// The console chrome: the grouped sidebar, the live header and the footer that
// carries the signed-in operator and the sign-out action.

import { LogOut } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import logoUrl from "@/assets/logo.png"
import { groupedAreas, type ConsoleArea } from "@/lib/areas"
import type { ConsoleSession } from "@/lib/console"
import { cn } from "@/lib/utils"

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
    <aside className="sticky top-0 hidden h-svh w-64 shrink-0 flex-col border-r border-border/60 bg-card/40 backdrop-blur-md md:flex">
      <div className="flex h-16 w-full items-center justify-center border-b border-border/60 px-4">
        <img
          src={logoUrl}
          alt="cookyourgame (cfg)"
          className="h-[52px] w-auto max-w-full object-contain drop-shadow-sm"
        />
      </div>

      <nav className="flex-1 space-y-4 overflow-y-auto px-3 py-4">
        {groupedAreas().map((group) => (
          <div key={group.group}>
            <div className="px-2 pb-1.5 text-[10px] font-semibold tracking-wider text-muted-foreground/70 uppercase">
              {group.group}
            </div>
            <div className="space-y-0.5">
              {group.areas.map((candidate) => {
                const Icon = candidate.icon
                const active = candidate.id === area.id
                return (
                  <button
                    key={candidate.id}
                    type="button"
                    onClick={() => onNavigate(candidate.id)}
                    className={cn(
                      "group flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-xs font-medium transition-all",
                      active
                        ? "bg-primary/10 text-primary shadow-xs ring-1 ring-primary/20"
                        : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                    )}
                  >
                    <Icon
                      className={cn(
                        "size-4 transition-colors",
                        active ? "text-primary" : "text-muted-foreground group-hover:text-foreground",
                      )}
                    />
                    <span className="truncate">{candidate.label}</span>
                  </button>
                )
              })}
            </div>
          </div>
        ))}
      </nav>

      <div className="border-t border-border/50 p-3">
        <div className="mb-2 rounded-md bg-background/40 px-3 py-2">
          <div className="flex items-center justify-between gap-2">
            <span className="truncate text-xs font-medium text-foreground">{session.username}</span>
            <Badge variant="outline">{session.role}</Badge>
          </div>
          {session.email !== undefined && session.email !== "" && (
            <p className="mt-0.5 truncate text-[10px] text-muted-foreground">{session.email}</p>
          )}
        </div>
        <Button variant="outline" size="sm" className="w-full" onClick={onLogout}>
          <LogOut />
          Sign out
        </Button>
      </div>
    </aside>
  )
}
