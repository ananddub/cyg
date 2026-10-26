// The console's route model. One shape covers both scopes, so the sidebar, the
// header and the page switch never have to agree on a string convention while
// the address bar keeps a deep-linkable hash:
//
//   #/games                 the games picker (landing)
//   #/users                 console administration
//   #/g/<gameId>/runtime    one game's workspace
//
// Parsing falls back rather than blanking: an unknown area keeps its scope's
// default, and an unknown shape lands on the picker.

import {
  GAME_DEFAULT_AREA,
  GAMES_AREA_ID,
  isGameArea,
  isGlobalArea,
} from "@/lib/areas"

export interface ConsoleRoute {
  /** `null` at the landing scope. */
  gameId: string | null
  areaId: string
}

export const GAMES_ROUTE: ConsoleRoute = { gameId: null, areaId: GAMES_AREA_ID }

/** A game workspace route; an area that does not belong to a game falls back to
 * the workspace default rather than rendering the landing's nav. */
export function gameRoute(gameId: string, areaId: string = GAME_DEFAULT_AREA): ConsoleRoute {
  return { gameId, areaId: isGameArea(areaId) ? areaId : GAME_DEFAULT_AREA }
}

export function routeHash(route: ConsoleRoute): string {
  if (route.gameId !== null) {
    return `#/g/${encodeURIComponent(route.gameId)}/${route.areaId}`
  }
  return `#/${isGlobalArea(route.areaId) ? route.areaId : GAMES_AREA_ID}`
}

export function parseRoute(hash: string): ConsoleRoute {
  const raw = hash.replace(/^#\/?/, "").replace(/\/+$/, "")
  const [head, id, area] = raw.split("/")
  if (head === "g" && id !== undefined && id !== "") {
    let gameId: string
    try {
      gameId = decodeURIComponent(id)
    } catch {
      return GAMES_ROUTE
    }
    return gameRoute(gameId, area ?? "")
  }
  if (isGlobalArea(raw)) return { gameId: null, areaId: raw }
  return GAMES_ROUTE
}
