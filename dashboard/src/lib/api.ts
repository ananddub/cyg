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

import type { ClientRow, Overview, RoomRow, TickSeries, WorkerRow, WsFrame } from "@/lib/types";

/** Every path this client knows, so a test can enumerate the surface. */
export const DASHBOARD_ENDPOINTS = {
  overview: "/api/dashboard/overview",
  rooms: "/api/dashboard/rooms",
  workers: "/api/dashboard/workers",
  clients: "/api/dashboard/clients",
  ticks: "/api/dashboard/ticks",
  ws: "/ws/dashboard",
} as const;

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

export function fetchOverview(): Promise<Overview> {
  return getJson<Overview>(DASHBOARD_ENDPOINTS.overview);
}

export function fetchRooms(): Promise<RoomRow[]> {
  return getJson<RoomRow[]>(DASHBOARD_ENDPOINTS.rooms);
}

export function fetchWorkers(): Promise<WorkerRow[]> {
  return getJson<WorkerRow[]>(DASHBOARD_ENDPOINTS.workers);
}

export function fetchClients(): Promise<ClientRow[]> {
  return getJson<ClientRow[]>(DASHBOARD_ENDPOINTS.clients);
}

export function fetchTicks(): Promise<TickSeries> {
  return getJson<TickSeries>(DASHBOARD_ENDPOINTS.ticks);
}

/** `wss:` on a secure page, since the socket is same-origin with the app. */
export function wsUrl(): string {
  const scheme = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${scheme}//${window.location.host}${DASHBOARD_ENDPOINTS.ws}`;
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
