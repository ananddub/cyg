import { useEffect } from "react"
import { Command } from "cmdk"
import {
  Gamepad2,
  Activity,
  Trophy,
  Database,
  Coins,
  Settings,
  SunMoon,
  LogOut,
  Workflow,
  Plus,
} from "lucide-react"
import { toggleTheme } from "@/lib/theme"

interface CommandMenuProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onNavigate?: (path: string) => void
  onLogout?: () => void
  onRegisterGame?: () => void
}

export function CommandMenu({
  open,
  onOpenChange,
  onNavigate,
  onLogout,
  onRegisterGame,
}: CommandMenuProps) {
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        onOpenChange(!open)
      }
    }
    const openMenu = () => onOpenChange(true)
    document.addEventListener("keydown", down)
    window.addEventListener("cyg:open-command-menu", openMenu)
    return () => {
      document.removeEventListener("keydown", down)
      window.removeEventListener("cyg:open-command-menu", openMenu)
    }
  }, [open, onOpenChange])

  const runCommand = (command: () => void) => {
    onOpenChange(false)
    command()
  }

  if (!open) return null

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/60 pt-24 backdrop-blur-xs"
      onClick={() => onOpenChange(false)}
    >
      <div
        className="w-full max-w-lg overflow-hidden rounded-xl border border-border bg-popover shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <Command className="flex flex-col overflow-hidden text-popover-foreground">
          <div className="flex items-center border-b border-border px-3">
            <Command.Input
              autoFocus
              placeholder="Search views, games, actions (Ctrl+K)..."
              className="flex h-11 w-full bg-transparent px-2 py-3 text-sm outline-none placeholder:text-muted-foreground"
            />
          </div>

          <Command.List className="max-h-80 overflow-y-auto p-2">
            <Command.Empty className="py-6 text-center text-xs text-muted-foreground">
              No results found.
            </Command.Empty>

            <Command.Group heading="Navigation" className="px-1.5 py-1 text-xs font-semibold text-muted-foreground">
              <Command.Item
                onSelect={() => runCommand(() => onNavigate?.("/overview"))}
                className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-foreground hover:bg-accent aria-selected:bg-accent"
              >
                <Gamepad2 className="size-4 text-violet-500" />
                <span>Games & Projects</span>
              </Command.Item>

              <Command.Item
                onSelect={() => runCommand(() => onNavigate?.("/audit"))}
                className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-foreground hover:bg-accent aria-selected:bg-accent"
              >
                <Activity className="size-4 text-sky-500" />
                <span>Audit Logs & Events</span>
              </Command.Item>

              <Command.Item
                onSelect={() => runCommand(() => onNavigate?.("/settings"))}
                className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-foreground hover:bg-accent aria-selected:bg-accent"
              >
                <Settings className="size-4 text-zinc-400" />
                <span>Console Settings</span>
              </Command.Item>
            </Command.Group>

            <Command.Group heading="Game Services" className="mt-2 px-1.5 py-1 text-xs font-semibold text-muted-foreground">
              <Command.Item
                onSelect={() => runCommand(() => onNavigate?.("/g/fab1ce0a-362e-48ad-900a-9a825912c54c/rules"))}
                className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-foreground hover:bg-accent aria-selected:bg-accent"
              >
                <Workflow className="size-4 text-amber-500" />
                <span>LiveOps Rules (tank-3d)</span>
              </Command.Item>

              <Command.Item
                onSelect={() => runCommand(() => onNavigate?.("/g/fab1ce0a-362e-48ad-900a-9a825912c54c/leaderboards"))}
                className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-foreground hover:bg-accent aria-selected:bg-accent"
              >
                <Trophy className="size-4 text-amber-400" />
                <span>Leaderboards & Tournaments</span>
              </Command.Item>

              <Command.Item
                onSelect={() => runCommand(() => onNavigate?.("/g/fab1ce0a-362e-48ad-900a-9a825912c54c/storage"))}
                className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-foreground hover:bg-accent aria-selected:bg-accent"
              >
                <Database className="size-4 text-emerald-500" />
                <span>Storage & Player Collections</span>
              </Command.Item>

              <Command.Item
                onSelect={() => runCommand(() => onNavigate?.("/g/fab1ce0a-362e-48ad-900a-9a825912c54c/monetization"))}
                className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-foreground hover:bg-accent aria-selected:bg-accent"
              >
                <Coins className="size-4 text-yellow-500" />
                <span>Economy & Monetization</span>
              </Command.Item>
            </Command.Group>

            <Command.Group heading="Quick Actions" className="mt-2 px-1.5 py-1 text-xs font-semibold text-muted-foreground">
              {onRegisterGame && (
                <Command.Item
                  onSelect={() => runCommand(onRegisterGame)}
                  className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-foreground hover:bg-accent aria-selected:bg-accent"
                >
                  <Plus className="size-4 text-emerald-500" />
                  <span>Register New Game</span>
                </Command.Item>
              )}

              <Command.Item
                onSelect={() => runCommand(toggleTheme)}
                className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-foreground hover:bg-accent aria-selected:bg-accent"
              >
                <SunMoon className="size-4 text-sky-400" />
                <span>Toggle Light / Dark Theme</span>
              </Command.Item>

              {onLogout && (
                <Command.Item
                  onSelect={() => runCommand(onLogout)}
                  className="flex cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-xs font-medium text-destructive hover:bg-destructive/10 aria-selected:bg-destructive/10"
                >
                  <LogOut className="size-4" />
                  <span>Sign out</span>
                </Command.Item>
              )}
            </Command.Group>
          </Command.List>
        </Command>
      </div>
    </div>
  )
}
