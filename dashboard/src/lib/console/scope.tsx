// The game a console view is looking at. Everything rendered under a game
// workspace reads its tenant from here, so a console read or write cannot
// silently address the whole install once an operator has picked a project.

import { createContext, useContext, type ReactNode } from "react"

const GameScopeContext = createContext<string | null>(null)

export function GameScopeProvider({
  gameId,
  children,
}: {
  gameId: string
  children: ReactNode
}) {
  return <GameScopeContext.Provider value={gameId}>{children}</GameScopeContext.Provider>
}

/** The scoped game id, or `null` on a landing/administration surface that is
 * deliberately install-wide. */
export function useGameScope(): string | null {
  return useContext(GameScopeContext)
}
