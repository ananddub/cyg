# Netcode — book/reference audit

Living checklist: every netcode claim is mapped to a code path and a test or a
known gap. Reference root: `/mnt/SSD/Books/Game Development/`.

Status: **PASS** = implemented and tested, **PARTIAL** = implemented with a
known limitation, **GAP** = not implemented.

## Core loop

| Reference | Technique | Status | Code | Evidence |
|---|---|---|---|---|
| Gambetta I, Gaffer *state_synchronization* | Authoritative server; clients send inputs, server owns state | PASS | `crates/engine-server/src/rooms/game_room/`, `host/tick_gate.rs` | input flow + host tests |
| Gaffer *snapshot_compression* | Server tick is fixed and independent of input arrival | PASS | `host/tick_gate.rs`, `host/tick_fanout.rs` | catch-up path covered by host tests |
| Gambetta II | Client-side prediction: apply input locally before ack | PASS | `cyg-core-rs/src/predict.rs`, `sdk/client/core-ts/src/core/engine.js` `CYGPredictDriver` | `engine-wasm/tests/prediction_parity.rs`, `sdk/client/web/test/predict.test.mjs` |
| Gambetta II | Server reconciliation: adopt authoritative state, replay unacked inputs | PASS | `CYGPredictDriver.onAck`, `cyg_reconcile` | prediction parity + predict tests |
| Gambetta II | Input sequence number echoed in ack | PASS | delta tail `acked u16`; `Room.inputSeq` / `ackedSeq` | netcode input-flow tests |
| Gaffer *networked_physics_2004* | Same quantizer on both sides; compare on wire lattice | PASS | `engine-schema` field encoding, descriptor hash, `Math.fround` reconcile | schema/descriptor and prediction parity tests |

## Remote entities

| Reference | Technique | Status | Code | Evidence |
|---|---|---|---|---|
| Gambetta III, Gaffer *snapshot_interpolation* | Snapshot ring; render remote entities in the past | PASS | `cyg-core-rs/src/interp.rs`, `CYGInterpolationTimeline` | `cargo test -p cyg-core-rs --lib interp::` |
| Gambetta III | Blend two bracketing snapshots; first sight snaps | PASS | `Interp::bracket`, `_interpolateAt` | 10 interp tests |
| Gaffer *snapshot_interpolation* | Clamp outside history; hold newest instead of blind extrapolation | PASS | `Interp::bracket`, JS stall branch | interp tests + JS timeline test |
| Gambetta III, Valve | Render delay / interpolation delay | PASS | `interpTicks` + jitter margin; Rust `delay_ticks` | JS/Rust interpolation tests |
| Gaffer *snapshot_interpolation* | Drop stale/reordered ticks, replace repeated tick | PASS | `Interp::push` | interp tests |
| — | Despawn immediately, no ghosts | PASS | `Interp::push` retain/newest logic | interp test |
| Gambetta III | Allow discrete fields (HP/state) to snap instead of lerp | PASS | `discrete` option in `CYGInterpolationTimeline` | JS timeline test |

## Lag compensation

| Reference | Technique | Status | Code | Evidence |
|---|---|---|---|---|
| Gambetta IV, Valve | Rewind historical transforms, query at shooter time | PASS | `engine-core/src/rewind/`, `rooms/rewind.rs`, `physics_lag.rs` | rewind interpolation + chaos tests |
| Valve | Keep ~1 s of history | PASS | `RewindManager::new(max_rewind_ms, tick_rate)`; host 256-frame bound | rewind tests |
| Valve | Step vs linear interpolation choice | PASS | `Interpolation::{Step, Linear}` | rewind tests |
| Gambetta IV | Use the shooter's input timestamp | PASS | input trailer carries `client_ms`; `InputFrame.clientTimeMs`, `clockMs()`, `Rewind.atClientMs` clamp future/old timestamps | stamp trailer test, rewind interpolation tests |
| Valve | Reject/clamp rewinds older than the window | PARTIAL | core manager clamps to oldest; host generic `RewindTrack` clamps too but has no time-window config | host rewind test |

## Wire

| Reference | Technique | Status | Code | Evidence |
|---|---|---|---|---|
| Gaffer *reading_and_writing_packets* | LSB-first bit writer/reader | PASS | `engine-webrtc-protocol/src/bits.rs` | packed tests |
| Gaffer *reading_and_writing_packets* | Quantize floats to N bits; integers exact | PASS | `engine-schema/src/encoding.rs`, `packed.rs` | schema + packed equivalence tests |
| Gaffer *snapshot_compression* | Delta from previous state, only changed fields | PASS | guest `DeltaWriter`, packed kinds 6/8 | packed lane tests |
| Quake III `msg.c` | Field presence mask / per-row update mask | PASS | packed patch `UPDATE mask u32` | packed tests |
| Gaffer *sending_large_blocks_of_data* | Fragment under MTU and reassemble | PASS | UDP delta fragments; WebRTC `0xA7` volatile chunks; SCTP reliable lane | net framing/chaos tests |
| Gaffer *sending_large_blocks_of_data* | Reliable large-block chunking | PARTIAL | descriptor/large reliable payloads use WS/DataChannel; no separate UDP chunk/ack system | no dedicated UDP large-block test |
| Gaffer *reliability_ordering...* | Sequence + redundant acks (SACK), RTO retransmit | PASS | `netcode/cyg.rs`, `plane/reliable/` | rudp/sack/ack tests |
| Gaffer *reliability_ordering...* | Congestion avoidance / bandwidth backoff | PASS | byte-based AIMD `cwnd`, halve on retransmit (once per PTO), ack-driven growth, token-bucket pacing, hard inflight/byte caps | `plane::reliable` cwnd/pacing/loss tests |
| RFC 9002 | PTO + RACK loss recovery | PASS | RFC 6298 SRTT/RTTVAR PTO, exponential backoff, RACK loss from selective acks | `plane::reliable::congestion`, `rack_marks_an_older_transmission_lost_after_a_newer_ack` |
| Gaffer *reliability_ordering...* | RTT/RTO estimation | PASS | `plane/reliable/pump.rs` | ack/RTO tests |

## Input

| Reference | Technique | Status | Code | Evidence |
|---|---|---|---|---|
| Valve, Gaffer | Newest-wins input sequence, stale drop | PASS | bridge `_inputLastSeq`; host `SeqWatermark` | input-flow tests |
| Valve | Hold last input / idle synthesis | PASS | `netcode/input.rs`, `synthesize_idle_inputs` | input-flow test |
| Gaffer/Valve | Redundant input frames for lossy lanes | PASS | `Room.inputRedundancy` (default 2) plus `Room.inputHistory` (default 2) replay of unacked schema frames in `__input_batch`; duplicate seqs are dropped | `input.test.mjs`, `parse_input_batch` tests |
| Gaffer | Input batching | PASS | `0x26` mesh batch + schema `__input_batch` (8 frames / 512 B cap, whole-batch rate charge) | host batch/limiter tests + web replay test |

## Clock / scope

| Reference | Technique | Status | Code | Evidence |
|---|---|---|---|---|
| Valve | Server tick clock / RTT/jitter tracking | PASS | `TimeSync`, delta tail echo | predict tests |
| Gaffer *state_synchronization* | Dirty flags, scope, per-client masking | PASS | packed dirty mask, `ViewMask`, AOI | packed/view tests |
| Gaffer *snapshot_compression* | Per-client priority / bandwidth budget | **GAP** | all dirty rows are sent; AOI only filters scope | no priority/budget test |

## Verification commands

```bash
cargo test -p cyg-core-rs --lib interp::
cargo test -p engine-core --test rewind_interpolation_test
cargo test -p engine-core --test proptest_rewind_chaos
cargo test -p engine-wasm --test prediction_parity
cargo test -p engine-net --lib transport::webrtc::contract
cargo test -p engine-server --lib netcode::tests::input_flow
cd sdk/client/web && node test/predict.test.mjs && node test/input.test.mjs
```

## Remaining work, in priority order

1. **Shooter timestamp end-to-end**: carry the client input timestamp into the
   guest so `Rewind.recordAt/atMs` uses the actual shot time, not just the
   server tick.
4. **Snapshot priority/budget**: per-client row priority when the dirty set is
   larger than the packet budget. Reference: Gaffer *snapshot_compression*.
5. **Reliable large-block chunking** for UDP if initial state can exceed one
   reliable message. Reference: Gaffer *sending_large_blocks_of_data*.
