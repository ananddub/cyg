// Mirror of `crates/engine-dashboard/src/lib.rs`. Keys are exactly what serde
// emits under `#[serde(rename_all = "camelCase")]`.
//
// The Rust types all derive `Default` and every field is serialised, so an idle
// server sends every key rather than omitting it. Where a number can mean "no
// measurement was taken" the field is optional (or `| null` for a Rust
// `Option`), and the UI must render an em dash for it — never 0.
//
// The API speaks microseconds. Any field ending in `Us` is microseconds; any
// field ending in `Ms` is milliseconds.
//
// There is deliberately no `rttMs` or jitter anywhere in this contract: the
// server has never measured a round trip, the client does.

/** The base-2 tick histogram. The wire also carries `bucketEdgesUs`, so a
 * chart never has to redo the arithmetic to label its axis. */
export type Histogram = number[];

/** The tenant this process hosts. `null` means identity is unresolved. */
export interface DashboardIdentity {
  game_id: string | null
  game_name: string | null
}

export interface ServerInfo {
  name: string;
  /** The game WebSocket/UDP port. */
  port: number;
  /** The port the control plane actually bound. */
  controlPort: number;
  webPort: number;
}

export interface Capacity {
  players: number;
  max: number;
}

/** Tick health, straight off the sim's status snapshot. */
export interface TickHealth {
  /** Measured tick rate over the publish window. */
  hz: number;
  /** What the project asked for. */
  configuredHz: number;
  /** Nominal period of one tick at the configured rate, µs. */
  nominalUs: number;
  /**
   * Optional because 0 here is not a measurement: while `hasSimSample` is
   * false this is the serde default, so the UI must show an em dash.
   */
  p50Us?: number;
  p99Us?: number;
  /** Ticks that overran `nominalUs` since the process started. */
  overruns: number;
  /** Ticks in the current window; 0 means "no samples yet". */
  samples: number;
  /** Publish counter: two polls with the same `seq` mean the sim stopped. */
  seq: number;
  histogram: Histogram;
}

export interface TransportMix {
  websocket: number;
  webrtc: number;
  nativeUdp: number;
  /** Live `/ws/signal` sockets. */
  signalPeers: number;
  signalsDelivered: number;
  signalsDropped: number;
}

export interface BusHealth {
  /** Queue depth. A number that only ever grows is a consumer that stopped. */
  inputsQueued: number;
  commandsQueued: number;
  deltasQueued: number;
  deltasSent: number;
  deltasDroppedFull: number;
  inputsDropped: number;
}

export interface SeatHealth {
  outstanding: number;
  minted: number;
  rejected: number;
  refused: number;
  bound: number;
  ttlSecs: number;
}

export interface RateLimitHealth {
  allowed: number;
  denied: number;
  squeezed: number;
  swept: number;
  tracked: number;
  rps: number;
  burst: number;
}

/** The whole first screen: one object, no joining on the client. */
export interface Overview {
  server: ServerInfo;
  /** Wall-clock seconds this control plane has been up. */
  uptimeSec: number;
  /** Concurrent connected clients across every transport. */
  ccu: number;
  /** Always 1 in this topology — one room per process. */
  rooms: number;
  capacity: Capacity;
  tick: TickHealth;
  transport: TransportMix;
  bus: BusHealth;
  seats: SeatHealth;
  rateLimit: RateLimitHealth;
  /** The Agones sidecar's own snapshot, passed through untouched. */
  agones: Record<string, unknown>;
  /** `false` until the sim has published once, in which case every `tick`
   * number is a serde default rather than a measurement. */
  hasSimSample: boolean;
  /** Age of `tick`. Meaningless while `hasSimSample` is false. */
  sampleAgeMs?: number;
}

export type RoomStatus = "live" | "empty";

export interface RoomRow {
  name: string;
  registerId?: string | null;
  gameId?: string | null;
  gameName?: string | null;
  clients: number;
  maxClients: number;
  tickHz: number;
  tickRate: number;
  port: number;
  controlPort: number;
  /** Seconds since the room last had a player; `null` while it has players. */
  emptyForSec: number | null;
  status: RoomStatus;
  hasSimSample: boolean;
  /** `null` until the sim has ever published — an absent sample has no age. */
  sampleAgeMs: number | null;
  /** What a client dials to reach this room, port-qualified. */
  signalingUrl: string;
  serverId: string;
}

export type WorkerName = "sim" | "control" | "transport";
/** How the plane is scheduled — decides whether a stall is priority or a slow handler. */
export type WorkerKind = "game" | "tokio" | "mio";

export interface WorkerRow {
  name: WorkerName;
  kind: WorkerKind;
  threads: number;
  ccu: number;
  /** `null` for planes with no tick of their own. */
  tickP50Us: number | null;
  tickP99Us: number | null;
  nominalTickUs: number | null;
  overruns: number | null;
  /** Deltas this plane accepted and dropped under backpressure. */
  deltasSent: number | null;
  deltasDropped: number | null;
  queueDepth: number | null;
  /** `false` until the sim publishes at least once. */
  live: boolean;
}

export type ClientTransport = "websocket" | "webrtc" | "native-udp";

export interface ClientRow {
  /** The transport's own session id: WS client id, WebRTC peer addr or UDP source. */
  session: string;
  transport: ClientTransport;
  room: string;
  /** `null` for a spectator. */
  playerId: number | null;
  /** Highest input seq applied — the number echoed back as `ackedSeq` in the delta tail. */
  ackedSeq: number | null;
  /** `null` for a session that has never sent one: connected but silent. */
  idleMs: number | null;
}

export interface TickPoint {
  /** Ms since the plane's monotonic epoch — the same clock as `sampleAgeMs`. */
  atMs: number;
  p50Us: number;
  p99Us: number;
  hz: number;
  players: number;
  seq: number;
  /** `true` when no sample reached this plane between two pushes. */
  gap: boolean;
}

/** The graph payload: the rolling series plus the newest window's histogram. */
export interface TickSeries {
  /** Seconds each sample spans (the sim publishes once per second). */
  intervalSec: number;
  points: TickPoint[];
  histogram: Histogram;
  /** Upper edge of each bucket in µs. */
  bucketEdgesUs: number[];
}

/** `/ws/dashboard` protocol this contract speaks; only `hello` negotiates it. */
export const WS_PROTOCOL = 1;

/** One `/ws/dashboard` message, tagged by `type` so a client can switch on it. */
export type WsFrame = WsHelloFrame | WsSnapshotFrame | WsGoodbyeFrame;

export interface WsHelloFrame {
  type: "hello";
  overview: Overview;
  ticks: TickSeries;
  /** A client that does not speak this version says so rather than guessing. */
  protocol: number;
}

export interface WsSnapshotFrame {
  type: "snapshot";
  overview: Overview;
  ticks: TickSeries;
}

/** The plane is closing the socket; `reason` is for the toast. */
export interface WsGoodbyeFrame {
  type: "goodbye";
  reason: string;
}
