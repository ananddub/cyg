// The dashboard's only network layer. Components and hooks call these; nothing
// else in `dashboard/src` opens a socket or hands a URL to `fetch`.
//
// Paths are root-absolute because the app is served under `/dashboard/` (vite
// `base`) while the API lives at the origin root — a bare relative path would
// resolve against the current directory and 404.
//
// Every failure rejects with the path in the message. A swallowed `null` would
// render as a zero, and a zero that was never measured is the one thing this
// dashboard must not show.

import type {
  ClientRow,
  DashboardIdentity,
  Overview,
  RoomRow,
  TickSeries,
  WorkerRow,
  WsFrame,
} from "@/lib/types";

/** Every path this client knows, so a test can enumerate the surface. */
export const DASHBOARD_ENDPOINTS = {
  identity: "/api/dashboard/identity",
  overview: "/api/dashboard/overview",
  rooms: "/api/dashboard/rooms",
  workers: "/api/dashboard/workers",
  clients: "/api/dashboard/clients",
  ticks: "/api/dashboard/ticks",
  ws: "/ws/dashboard",
} as const;

/** The workspace's game id rides on every telemetry read so the server can
 * refuse a game this process does not host rather than send its own numbers. */
function scoped(path: string, gameId: string | null): string {
  if (gameId === null || gameId === "") return path;
  return `${path}?${new URLSearchParams({ game_id: gameId }).toString()}`;
}

/** The control plane answers in well under a second; longer than this is dead. */
const REQUEST_TIMEOUT_MS = 4000;

async function getJson<T>(path: string): Promise<T> {
  let response: Response;
  try {
    response = await fetch(path, {
      method: "GET",
      headers: { accept: "application/json" },
      signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
    });
  } catch (cause) {
    throw new Error(`GET ${path} failed: ${describe(cause)}`, { cause });
  }
  if (!response.ok) {
    throw new Error(`GET ${path} returned ${response.status} ${response.statusText}`);
  }
  try {
    return (await response.json()) as T;
  } catch (cause) {
    throw new Error(`GET ${path} returned unparseable JSON: ${describe(cause)}`, { cause });
  }
}

function describe(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

export function fetchIdentity(): Promise<DashboardIdentity> {
  return getJson<DashboardIdentity>(DASHBOARD_ENDPOINTS.identity);
}

export function fetchOverview(gameId: string | null): Promise<Overview> {
  return getJson<Overview>(scoped(DASHBOARD_ENDPOINTS.overview, gameId));
}

export function fetchRooms(gameId: string | null): Promise<RoomRow[]> {
  return getJson<RoomRow[]>(scoped(DASHBOARD_ENDPOINTS.rooms, gameId));
}

export function fetchWorkers(gameId: string | null): Promise<WorkerRow[]> {
  return getJson<WorkerRow[]>(scoped(DASHBOARD_ENDPOINTS.workers, gameId));
}

export function fetchClients(gameId: string | null): Promise<ClientRow[]> {
  return getJson<ClientRow[]>(scoped(DASHBOARD_ENDPOINTS.clients, gameId));
}

export function fetchTicks(gameId: string | null): Promise<TickSeries> {
  return getJson<TickSeries>(scoped(DASHBOARD_ENDPOINTS.ticks, gameId));
}

/** `wss:` on a secure page, since the socket is same-origin with the app. */
export function wsUrl(gameId: string | null): string {
  const scheme = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${scheme}//${window.location.host}${scoped(DASHBOARD_ENDPOINTS.ws, gameId)}`;
}

/** Narrows a raw socket message to a frame, or `null` if it is not one. */
export function parseWsFrame(raw: string): WsFrame | null {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null) return null;
  const type = (value as { type?: unknown }).type;
  if (type !== "hello" && type !== "snapshot" && type !== "goodbye") return null;
  return value as WsFrame;
}
