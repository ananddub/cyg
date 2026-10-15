// The console session, kept in `sessionStorage` so a tab's credential dies with
// the tab. One module owns it: the login writes, the logout clears, and `401`
// does too, so there is no path where the UI stays on an authenticated screen
// holding a token the server has stopped accepting.

import { useSyncExternalStore } from "react"

import type { ConsoleSession } from "@/lib/console"

const STORAGE_KEY = "openoxide.console.session"

let current: ConsoleSession | null = read()
const listeners = new Set<() => void>()

function read(): ConsoleSession | null {
  try {
    const raw = window.sessionStorage.getItem(STORAGE_KEY)
    if (raw === null) return null
    const parsed = JSON.parse(raw) as ConsoleSession
    return typeof parsed?.token === "string" && parsed.token !== "" ? parsed : null
  } catch {
    // An unparseable or unavailable store is simply "not signed in".
    return null
  }
}

function emit(): void {
  for (const listener of listeners) listener()
}

/** The live session, or `null`. Stable identity, so it is safe as a snapshot. */
export function session(): ConsoleSession | null {
  return current
}

export function saveSession(next: ConsoleSession): void {
  current = next
  try {
    window.sessionStorage.setItem(STORAGE_KEY, JSON.stringify(next))
  } catch {
    // A browser that refuses storage still gets a working in-memory session.
  }
  emit()
}

export function clearSession(): void {
  if (current === null) return
  current = null
  try {
    window.sessionStorage.removeItem(STORAGE_KEY)
  } catch {
    // Nothing to undo: the in-memory copy is already gone.
  }
  emit()
}

export function useSession(): ConsoleSession | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    session,
    session,
  )
}
