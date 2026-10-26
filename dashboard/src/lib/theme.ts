// The UI theme, kept in `localStorage` so it survives reloads and new tabs —
// unlike the session, which is deliberately per-tab. One module owns it: the
// toggle writes, `applyTheme` mirrors the choice onto <html>, and everything
// else reads through the hook. Dark is the default because it is the palette
// the console was drawn in.

import { useSyncExternalStore } from "react"

export type Theme = "light" | "dark"

const STORAGE_KEY = "cyg.console.theme"

let current: Theme = read()
const listeners = new Set<() => void>()

function read(): Theme {
  try {
    return window.localStorage.getItem(STORAGE_KEY) === "light" ? "light" : "dark"
  } catch {
    return "dark"
  }
}

function emit(): void {
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** Puts the theme on <html> and persists it. Safe before the first render. */
export function applyTheme(next: Theme): void {
  current = next
  document.documentElement.classList.toggle("dark", next === "dark")
  try {
    window.localStorage.setItem(STORAGE_KEY, next)
  } catch {
    // A browser that refuses storage still gets a working in-memory theme.
  }
  emit()
}

export function toggleTheme(): void {
  applyTheme(current === "dark" ? "light" : "dark")
}

/** The live theme. Stable identity, so it is safe as a snapshot. */
export function theme(): Theme {
  return current
}

export function useTheme(): Theme {
  return useSyncExternalStore(subscribe, theme, theme)
}
