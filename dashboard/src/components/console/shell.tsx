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
    <aside className="sticky top-0 hidden h-svh w-64 shrink-0 flex-col border-r border-border bg-sidebar md:flex">
      <div className="flex h-16 w-full items-center justify-center border-b border-border px-4">
        <img
          src={logoUrl}
          alt="cookyourgame (cfg)"
          className="h-[52px] w-auto max-w-full object-contain drop-shadow-sm"
        />
      </div>

      <nav className="flex-1 space-y-4 overflow-y-auto px-3 py-4">
        {groupedAreas().map((group) => (
          <div key={group.group}>
            <div className="px-2 pb-1.5 text-[10px] font-semibold tracking-wider text-muted-foreground uppercase">
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
                      "group flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-xs font-medium transition-colors",
                      active
                        ? "bg-[#27272a] text-[#fafafa] font-semibold shadow-xs border border-[#3f3f46]/60"
                        : "text-[#a1a1aa] hover:bg-[#18181b] hover:text-[#fafafa]",
                    )}
                  >
                    <Icon
                      className={cn(
                        "size-4 shrink-0 transition-colors",
                        active ? "text-[#fafafa]" : "text-[#71717a] group-hover:text-[#fafafa]",
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

      <div className="border-t border-border p-3">
        <div className="mb-2 rounded-md bg-muted/40 px-3 py-2 border border-border/50">
          <div className="flex items-center justify-between gap-2">
            <span className="truncate text-xs font-medium text-foreground">{session.username}</span>
            <Badge variant="outline" className="border-border text-[10px]">{session.role}</Badge>
          </div>
          {session.email !== undefined && session.email !== "" && (
            <p className="mt-0.5 truncate text-[10px] text-muted-foreground">{session.email}</p>
          )}
        </div>
        <Button variant="outline" size="sm" className="w-full border-border hover:bg-muted" onClick={onLogout}>
          <LogOut />
          Sign out
        </Button>
      </div>
    </aside>
  )
}
