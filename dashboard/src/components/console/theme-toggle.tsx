// The light/dark switch. One button that shows the mode it will take you to —
// a sun in the dark, a moon in the light — the way the reference kit does it.

import { Moon, Sun } from "lucide-react"

import { Button } from "@/components/ui/button"
import { toggleTheme, useTheme } from "@/lib/theme"

export function ThemeToggle() {
  const mode = useTheme()
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label={mode === "dark" ? "Switch to light mode" : "Switch to dark mode"}
      title={mode === "dark" ? "Light mode" : "Dark mode"}
      onClick={toggleTheme}
    >
      {mode === "dark" ? <Sun /> : <Moon />}
    </Button>
  )
}
