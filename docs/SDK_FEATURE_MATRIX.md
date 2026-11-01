# SDK Feature Matrix — server surface vs every SDK

Last audited: Nov 1, 2026. Server surface = what the host + control plane
actually implement. "tested" = automated test that runs in `just test-sdks`
or the Rust suites. This file is the contract for what "SDK complete" means.

## Server surface (the features SDKs must cover)

| Feature | Server lane | Notes |
|---|---|---|
| Matchmake (joinOrCreate/join/joinSpectator/reconnect) | WS lane HTTP + control plane | seat JSON: sessionId, reconnectionToken, gamePort, udpPort |
| Room socket (state, schema descriptor, patches) | WS lane / WebRTC DataChannel / native UDP | WS = proto 14/15/17, WebRTC = lane-classified |
| Input + reconciliation ack | all lanes | 23B mesh frame; acked seq echoed in delta tail |
| Schema state decode (OXS2 world kind 3/4) | all lanes | descriptor = cyg.schema.bin (served on upgrade) |
| Typed app messages (`send`/`onMessage`) | all lanes | `[0x05][type_len][type][payload]` |
| Events (reliable ordered) | WebRTC reliable lane + WS | 15B event frame |
| Batched inputs | WebRTC/UDP | OP_INPUT_BATCH 0x26 |
| Clock sync / server tick | delta tail (tick) + PING 18 | |
| Reconnection (seat hold + signed token) | control plane + WS retained map | |
| Stale-socket takeover | host Connected duplicate branch | |
| Voice (mesh relay) | WebRTC/UDP | VOICE_MAGIC 0x56 |
| Spectator mode | all lanes | |
| Graceful shutdown (`server_closing`) | broadcast event | |
| maxMessagesPerSecond shield | host inbound | |

## Per-SDK state

| Feature | JS/Web | Godot | Unity/C# | Unreal |
|---|---|---|---|---|
| Matchmake + room socket | ✅ tested | ✅ tested (live e2e) | ⚠️ live e2e built, blocked on server UDP-join deny | ✅ implemented (16.6k-line plugin: CYGClient, CookYourGame editor module, profiler, resilience cookies) |
| Schema decode (OXS2) | ✅ tested | ✅ tested (decoder + e2e) | ❌ untested | ⚠️ |
| Input + acks | ✅ tested | ✅ e2e | ❌ | ⚠️ |
| App messages | ✅ tested | ⚠️ unit only | ❌ | ⚠️ |
| Events | ✅ | ⚠️ unit only | ❌ | ⚠️ |
| WebRTC DataChannel lane | ✅ tested (loss/chaos suites) | ➖ WS-only (no GDExtension WebRTC assumed) | ❌ | ❌ |
| WS lane | ✅ fallback | ✅ **primary, e2e green** | ❌ | ⚠️ |
| Reconnection | ✅ | ⚠️ client auto-reconnect exists, not e2e-tested | ❌ | ❌ |
| Interpolation/replay helpers | ✅ | ✅ units | ❌ | ❌ |
| Voice | ✅ | ⚠️ pass-through units | ❌ | ❌ |

## Gaps that block "SDK complete"

1. **Server WS lane feature depth**: events + reconnection + batched inputs
   over the WS lane reuse the rtc framing on the wire, but no automated test
   drives them through `ws_port` yet (only state/input/leave are pinned).
2. **Unity/C# runtime**: decoder contract tests green (12 checks, zero-dep
   `dotnet run`). The live native-UDP flow is built (CYG_E2E_ORIGIN env) and
   has already fixed three SDK bugs (seat host default, udpPort vs gamePort
   lane, naive JSON extractor breaking on unquoted numbers) — it currently
   pins a SERVER-side gap: the UDP plane DENYs fresh control-plane tokens by
   resolving them against a stale retained session. Until that is fixed the
   live part stays env-gated.
3. **Unreal**: implementation is a full plugin (16.6k lines: client, serialization packers, transform/attachment/animator sync, visibility, session-cookie resilience, network profiler, editor tooling). What it lacks is AUTOMATED TESTS — UE automation tests (`IMPLEMENT_SIMPLE_AUTOMATION_TEST`) against the same wire vectors are the missing piece; a UE build environment is required to run them.
4. **3D example coverage**: sky3d (3D, batched 3D ABI) should get the same
   live e2e treatment as coin-rush.
5. **Multi-feature e2e per SDK**: one flow per feature (auth, storage, RPC)
   against a dedicated fixture room, not just coin-rush.
