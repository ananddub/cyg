// Client core: transports, room lifecycle, schema decode, prediction and wire codecs.
// TODO(core-ts): split into transport/room/voice/predict modules (keep behavior 1:1).
const factory = () => {
    // ['W'][version u8][kind u8][from u32][to u32][opaque JSON body], big-endian.
    const VOICE_MAGIC = 0x56;
    const SIGNAL_MAGIC = 0x57;

    /** Normalise a WebSocket payload (ArrayBuffer / TypedArray / Buffer) to a Uint8Array view. */
    function toBinaryBytes(data) {
        if (!data) return null;
        if (data instanceof ArrayBuffer) return new Uint8Array(data);
        if (data.buffer) return new Uint8Array(data.buffer, data.byteOffset || 0, data.byteLength || data.length);
        if (typeof Buffer !== 'undefined' && Buffer.isBuffer && Buffer.isBuffer(data)) return new Uint8Array(data);
        return null;
    }
    const SIGNAL_VERSION = 1;
    const SIGNAL_HEADER_LEN = 11;
    const SIGNAL_BROADCAST = 0xFFFFFFFF;

    const SignalKind = {
        JOIN: 1, OFFER: 2, ANSWER: 3, ICE: 4, BYE: 5,
        CONFIG: 8, PEER_JOINED: 9, PEER_LEFT: 10, SIGNAL: 11,
        ERROR: 12, MUTED: 13
    };

    const _encoder = (typeof TextEncoder !== 'undefined') ? new TextEncoder() : null;
    const _decoder = (typeof TextDecoder !== 'undefined') ? new TextDecoder() : null;

    // --- cyg-core-rs wasm core (THE schema decoder) -------------------------
    // Every OXS2 decode, the descriptor parse and the schema-input packing
    // live in `cyg_core_rs.wasm` (crates/cyg-core-rs, C ABI via wasm_ffi.rs).
    // cyg.js keeps transports/reconnect/voice/REST and re-diffs the core's
    // state to drive the Colyseus-style event callbacks.

    /** engine_schema::WireKind discriminant -> cyg.js field type name. */
    const CORE_KIND_TO_TYPE = [
        'u8', 'i8', 'u16', 'i16', 'u32', 'i32', 'f32', 'f64', 'bool', 'i64',
        'string', 'array'
    ];

    class WasmCore {
        constructor(ex) {
            this.ex = ex;
            this.mem = ex.memory;
            if (ex.cyg_abi_version() !== 1) {
                throw new Error('cyg-core wasm ABI mismatch — rebuild cyg_core_rs.wasm');
            }
            this._arena = null;
        }

        /** Bump allocator over linear memory. The core never retains these
         *  bytes (it copies on read/write inside one call), so the arena is
         *  reset whenever it fills up. */
        _alloc(n) {
            if (!this._arena || this._arena.used + n > this._arena.cap) {
                const pages = Math.max(1, Math.ceil((n + 16) / 65536));
                if (this.mem.grow(pages) === -1) {
                    throw new Error('cyg-core wasm memory exhausted');
                }
                const cap = pages * 65536;
                this._arena = { start: this.mem.buffer.byteLength - cap, cap, used: 0 };
            }
            // Typed-array views require alignment: an f64/i64 block must start
            // on an 8-byte boundary or the constructor throws.
            this._arena.used = (this._arena.used + 7) & ~7;
            const at = this._arena.start + this._arena.used;
            this._arena.used += n;
            return at;
        }

        _write(bytes) {
            const at = this._alloc(bytes.length);
            new Uint8Array(this.mem.buffer, at, bytes.length).set(bytes);
            return at;
        }

        _readStr(at, n) {
            return _decoder.decode(new Uint8Array(this.mem.buffer, at, n));
        }

        _ex() {
            if (!this.ex) throw new Error('cyg-core wasm closed');
            return this.ex;
        }

        create(descriptorBytes) {
            const ex = this._ex();
            const at = this._write(descriptorBytes);
            return ex.cyg_create(at, descriptorBytes.length);
        }

        free(handle) {
            if (handle) this.ex.cyg_free(handle);
        }

        /** Apply one raw room-data payload (with the [0xA5 "delta"] prefix);
         *  returns the event count, or -1 when the payload is not a schema
         *  delta (game delta / voice / signal frames fall through to JS). */
        apply(handle, payloadBytes) {
            const ex = this._ex();
            const at = this._write(payloadBytes);
            return ex.cyg_apply_payload(handle, at, payloadBytes.length);
        }

        stateReady(handle) { return this._ex().cyg_state_ready(handle) === 1; }

        schemaCount(handle) { return this._ex().cyg_schema_count(handle); }
        schemaName(handle, i) {
            const ex = this._ex(); const at = this._alloc(128);
            const n = ex.cyg_schema_name(handle, i, at, 128);
            return n < 0 ? null : this._readStr(at, Math.min(n, 128));
        }
        schemaHash(handle, i) { return this._ex().cyg_schema_hash(handle, i) >>> 0; }
        schemaVersion(handle, i) { return this._ex().cyg_schema_version(handle, i); }
        schemaFieldCount(handle, i) { return this._ex().cyg_schema_field_count(handle, i); }
        schemaFieldName(handle, i, f) {
            const ex = this._ex(); const at = this._alloc(128);
            const n = ex.cyg_schema_field_name(handle, i, f, at, 128);
            return n < 0 ? null : this._readStr(at, Math.min(n, 128));
        }
        schemaFieldKind(handle, i, f) { return this._ex().cyg_schema_field_kind(handle, i, f); }
        schemaFieldFlags(handle, i, f) { return this._ex().cyg_schema_field_flags(handle, i, f); }
        schemaFieldElementKind(handle, i, f) {
            return this._ex().cyg_schema_field_element_kind(handle, i, f);
        }
        schemaFieldNested(handle, i, f) {
            const ex = this._ex(); const at = this._alloc(128);
            const n = ex.cyg_schema_field_nested(handle, i, f, at, 128);
            return n < 0 ? null : this._readStr(at, Math.min(n, 128));
        }
        nestedCount(handle) {
            return typeof this._ex().cyg_nested_count === 'function'
                ? this._ex().cyg_nested_count(handle) : 0;
        }
        _nestedPart(fn, handle, i) {
            const ex = this._ex(); const at = this._alloc(128);
            const n = fn.call(ex, handle, i, at, 128);
            return n < 0 ? null : this._readStr(at, Math.min(n, 128));
        }
        nestedCollection(handle, i) {
            return this._nestedPart(this._ex().cyg_nested_collection, handle, i);
        }
        nestedParent(handle, i) {
            return this._nestedPart(this._ex().cyg_nested_parent, handle, i);
        }
        nestedField(handle, i) {
            return this._nestedPart(this._ex().cyg_nested_field, handle, i);
        }

        collectionCount(handle) { return this._ex().cyg_collection_count(handle); }
        collectionSchemaHash(handle, i) {
            return this._ex().cyg_collection_schema_hash(handle, i) >>> 0;
        }
        collectionName(handle, i) {
            const ex = this._ex(); const at = this._alloc(128);
            const n = ex.cyg_collection_name(handle, i, at, 128);
            return n < 0 ? null : this._readStr(at, Math.min(n, 128));
        }

        entityCountIn(handle, ci) { return this._ex().cyg_entity_count_in(handle, ci); }
        entityIdAtIn(handle, ci, pos) { return this._ex().cyg_entity_id_at_in(handle, ci, pos); }
        getF64In(handle, ci, entityId, index) {
            return this._ex().cyg_get_f64_in(handle, ci, entityId, index);
        }
        getI64(handle, entityId, index) {
            return this._ex().cyg_get_i64_by_index(handle, entityId, index);
        }
        getBoolIn(handle, ci, entityId, index) {
            return this._ex().cyg_get_bool_in(handle, ci, entityId, index);
        }
        getStringIn(handle, ci, entityId, index) {
            const ex = this._ex();
            const at = this._alloc(256);
            const n = ex.cyg_get_string_in(handle, ci, entityId, index, at, 256);
            if (n < 0) return null;
            return this._readStr(at, Math.min(n, 256));
        }

        rootFieldCount(handle) { return this._ex().cyg_root_field_count(handle); }
        rootFieldName(handle, i) {
            const ex = this._ex(); const at = this._alloc(128);
            const n = ex.cyg_root_field_name(handle, i, at, 128);
            return n < 0 ? null : this._readStr(at, Math.min(n, 128));
        }
        rootFieldKind(handle, i) { return this._ex().cyg_root_field_kind(handle, i); }
        rootSchemaName(handle) {
            const ex = this._ex(); const at = this._alloc(128);
            const n = ex.cyg_root_schema_name(handle, at, 128);
            return n < 0 ? null : this._readStr(at, Math.min(n, 128));
        }
        rootSchemaVersion(handle) { return this._ex().cyg_root_schema_version(handle); }
        getRootF64(handle, index) { return this._ex().cyg_get_root_f64(handle, index); }
        getRootBool(handle, index) { return this._ex().cyg_get_root_bool(handle, index); }
        getRootString(handle, index) {
            const ex = this._ex(); const at = this._alloc(256);
            const n = ex.cyg_get_root_string(handle, index, at, 256);
            if (n < 0) return null;
            return this._readStr(at, Math.min(n, 256));
        }

        fieldIndex(handle, name) {
            const ex = this._ex();
            const bytes = _encoder.encode(name);
            const at = this._write(bytes);
            return ex.cyg_field_index(handle, at, bytes.length);
        }

        /** Encode a schema-lane input; `values` = raw i64 bit patterns in the
         *  Input schema's field order (f32/f64 pass IEEE bits). */
        encodeSchemaInput(handle, seq, values) {
            const ex = this._ex();
            const n = values.length;
            const inAt = this._alloc(n * 8);
            const i64 = new BigInt64Array(this.mem.buffer, inAt, n);
            for (let i = 0; i < n; i++) i64[i] = values[i];
            const outAt = this._alloc(512);
            const lenAt = this._alloc(8);
            const lenView = new DataView(this.mem.buffer, lenAt, 8);
            const rc = ex.cyg_encode_schema_input(handle, seq, inAt, n, outAt, 512, lenAt);
            if (rc !== 0) return null;
            const len = Number(lenView.getBigUint64(0, true));
            return new Uint8Array(this.mem.buffer, outAt, len).slice();
        }
    }

    let _coreLoadPromise = null;
    /**
     * Load `cyg_core_rs.wasm` (shipped next to cyg.js; override with the
     * `CYG_CORE_WASM_URL` global in the browser). One wasm instance serves
     * every room: each room gets its own core handle from `cyg_create`.
     */
    function loadCygCore() {
        if (_coreLoadPromise) return _coreLoadPromise;
        _coreLoadPromise = (async () => {
            let bytes = null;
            const overrideUrl = typeof CYG_CORE_WASM_URL === 'string' ? CYG_CORE_WASM_URL : null;
            const isNode = typeof process !== 'undefined' && process.versions
                && process.versions.node && typeof require === 'function';
            if (isNode) {
                try {
                    bytes = require('fs').readFileSync(
                        require('path').join(__dirname, 'cyg_core_rs.wasm'));
                } catch (_) { /* fall through */ }
            }
            if (!bytes && typeof fetch === 'function') {
                try {
                    const url = overrideUrl || 'cyg_core_rs.wasm';
                    bytes = new Uint8Array(await (await fetch(url)).arrayBuffer());
                } catch (e) {
                    throw new Error('cyg_core_rs.wasm not found — ship it next to cyg.js'
                        + ' (or set CYG_CORE_WASM_URL): ' + (e && e.message || e));
                }
            }
            if (!bytes) {
                throw new Error('cyg_core_rs.wasm not found — ship it next to cyg.js');
            }
            const { instance } = await WebAssembly.instantiate(bytes, {});
            return new WasmCore(instance.exports);
        })();
        return _coreLoadPromise;
    }

    function encodeSignalFrame(kind, to, body) {
        // `from` is deliberately absent: the server stamps the sender from its own
        // session map, so a client cannot claim to be somebody else.
        const json = (typeof body === 'string') ? body : JSON.stringify(body || {});
        const bodyBytes = _encoder ? _encoder.encode(json) : new Uint8Array(0);
        const buf = new Uint8Array(SIGNAL_HEADER_LEN + bodyBytes.length);
        const view = new DataView(buf.buffer);
        buf[0] = SIGNAL_MAGIC;
        buf[1] = SIGNAL_VERSION;
        buf[2] = kind;
        view.setUint32(3, 0, false);
        view.setUint32(7, to, false);
        buf.set(bodyBytes, SIGNAL_HEADER_LEN);
        return buf;
    }

    /** Returns `{ kind, from, to, body }`, or null when this is not a signal frame. */
    function decodeSignalFrame(bytes) {
        if (!bytes || bytes.length < SIGNAL_HEADER_LEN || bytes[0] !== SIGNAL_MAGIC) return null;
        if (bytes[1] !== SIGNAL_VERSION) return null;
        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        const raw = bytes.length > SIGNAL_HEADER_LEN && _decoder
            ? _decoder.decode(bytes.subarray(SIGNAL_HEADER_LEN))
            : '';
        let body = null;
        try { body = raw ? JSON.parse(raw) : {}; } catch (e) { body = null; }
        return {
            version: bytes[1],
            kind: bytes[2],
            from: view.getUint32(3, false),
            to: view.getUint32(7, false),
            body: body
        };
    }

    // --- MESH GAME DATA WIRE CODEC (mirrors engine_webrtc_protocol) ---
    // Little-endian and opcode-first, so one compare dispatches a datagram:
    //   0x01 input [seq u16][flags u32][aimYaw f32][aimPitch f32][moveX f32][moveZ f32]
    //   0x02 state [tick u16][count u8] + count x 19B records
    //   0x03 event [seq u16][kind u8][from u16][target u16][tick u16][value f32]
    //   0x56 voice a VoicePacket verbatim, so the relay buffer needs no rewrite
    const MeshOp = { INPUT: 0x01, STATE: 0x02, EVENT: 0x03, VOICE: VOICE_MAGIC };
    const MeshEventKind = { FIRE: 1, HIT: 2, DAMAGE: 3, KILL: 4, RESPAWN: 5, EFFECT: 6 };

    // CYG app messages (typed, reliable, client <-> server):
    // [0x05][type_len u8][UTF-8 type][opaque payload]
    // Deliberately independent of Colyseus/msgpack. Input (0x01), mesh events
    // (0x03), voice (0x56), signaling (0x57) and spectator (0xFF) keep their
    // existing frames.
    const AppOp = { MESSAGE: 0x05 };
    const APP_MESSAGE_MAX_TYPE = 255;
    const MESH_INPUT_LEN = 23;
    const MESH_EVENT_LEN = 14;
    const MESH_STATE_HEADER_LEN = 4;
    const MESH_RECORD_LEN = 19;
    const MESH_MAX_FRAME = 1200;
    const MESH_MAX_ENTITIES = Math.floor((MESH_MAX_FRAME - MESH_STATE_HEADER_LEN) / MESH_RECORD_LEN);
    const MESH_LEN_SCALE = 16;
    const MESH_ANGLE_SCALE = 32768;
    const MESH_TAU = Math.PI * 2;

    // Rust rounds half away from zero; Math.round rounds half up, which would make
    // the two decoders disagree on every exact .5.
    function roundHalfAway(v) { return v < 0 ? -Math.round(-v) : Math.round(v); }

    // fround first, then scale: multiplying an f32 by 16 is exact, so the f64
    // product matches Rust's f32 one bit for bit. Non-finite input saturates to
    // the same extremes (or 0 for NaN) as the Rust codec.
    function quantizeLen(v) {
        const scaled = roundHalfAway(Math.fround(v) * MESH_LEN_SCALE);
        return Math.max(-32768, Math.min(32767, scaled)) | 0;
    }

    // Angle scaling runs on f64 here and f32 in Rust, so the two can differ by one
    // least-significant step — 0.0055 degrees, far below what a camera can show.
    function quantizeAngle(v) {
        let a = Math.fround(v) % MESH_TAU;
        if (a > Math.PI) a -= MESH_TAU;
        else if (a < -Math.PI) a += MESH_TAU;
        const scaled = roundHalfAway(a / MESH_TAU * 2 * MESH_ANGLE_SCALE);
        return Math.max(-32768, Math.min(32767, scaled)) | 0;
    }

    const dequantizeLen = (raw) => raw / MESH_LEN_SCALE;
    const dequantizeAngle = (raw) => raw * (MESH_TAU / (2 * MESH_ANGLE_SCALE));

    /** Newest-wins over a wrapping u16 sequence, identical to the Rust rule. */
    function isNewerSeq(last, value) {
        const delta = (value - last) & 0xFFFF;
        return delta !== 0 && delta < 0x8000;
    }

    // --- SERVER CHANNEL LAYER (mirrors engine_webrtc_protocol::channel) ---
    // The star half of the protocol: the server pushing its per-tick delta over
    // `game-unreliable`, where arriving out of order is the normal case, not the
    // exception. `ordered: false` means tick 41 can land before tick 40, and
    // `maxRetransmits: 0` means a lost chunk is gone forever — so every delta
    // carries a room-wide `seq`, and anything superseded is dropped unread.
    const ChannelOp = { DELTA: 0x04, ACK: 0x06 };
    const AckKind = { EVENT: 1 };
    const DELTA_HEADER_LEN = 9; // [op][seq u16][tick u32][fragIdx u8][fragTotal u8]
    const ACK_WIRE_LEN = 4;     // [op][seq u16][kind u8]
    const MAX_DELTA_CHUNK = MESH_MAX_FRAME - DELTA_HEADER_LEN;
    const MAX_FRAGMENTS = 255;  // width of frag_total (u8), parity with Rust
    const FRAGMENT_TTL_MS = 3000;
    // Phase D reconciliation tail the server appends to every room delta:
    // [acked_seq u16][server_tick u32][client_stamp u16]. `wrap_delta_into`
    // writes it, this reads it. The 2-byte stamp is OUR rotating marker from
    // the last `__input` frame — the server echoes it back and the echo pair
    // measures RTT without a dedicated ping lane. DELTA_TAIL_LEN stays 6 for
    // OLD servers (no echo): the reader falls back when the stamp is unknown.
    const DELTA_TAIL_LEN = 8;
    // Client RTT stamp trailer appended to every schema `__input` frame:
    // [0xC7 0x59][stamp u16 LE] (the magic lets the host peel it without
    // knowing the schema-encoded frame length). Mirrors
    // engine-server's `netcode::extract_input_stamp`.
    const INPUT_STAMP_MAGIC = [0xC7, 0x59];
    const INPUT_STAMP_LEN = 8;
    // RTT smoothing: EMA alpha 0.125 (the classic RFC 793/3550 choice — one
    // sample per delta, so the estimate settles in ~8 deltas) and jitter as
    // an RFC 3550-style mean deviation of the inter-sample deltas.
    const RTT_EMA_ALPHA = 0.125;
    const JITTER_DEV_ALPHA = 0.125;
    // How many half-assembled deltas may sit at once. A supersession normally
    // retires them within a tick or two; more than this means a stalled link,
    // and the excess is evicted rather than growing without bound.
    const MAX_INFLIGHT_DELTAS = 8;

    /** `null` for anything that is not a well-formed delta frame. */
    function parseDeltaFrame(bytes) {
        if (!bytes || bytes.length < DELTA_HEADER_LEN || bytes[0] !== ChannelOp.DELTA) return null;
        // Rust `send_frame`/`split_delta` refuse anything past the path MTU, so a
        // longer frame is either corruption or a hostile sender. Accepting it
        // would let one grow the reassembly buffer far beyond what the wire
        // contract allows.
        if (bytes.length > MESH_MAX_FRAME) return null;
        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        const fragIdx = bytes[7];
        const fragTotal = bytes[8];
        if (fragTotal === 0 || fragIdx >= fragTotal) return null;
        return {
            seq: view.getUint16(1, true),
            tick: view.getUint32(3, true),
            fragIdx,
            fragTotal,
            chunk: bytes.subarray(DELTA_HEADER_LEN),
        };
    }

    function encodeDeltaFrame(seq, tick, fragIdx, fragTotal, chunk) {
        const buf = new Uint8Array(DELTA_HEADER_LEN + chunk.length);
        const view = new DataView(buf.buffer);
        buf[0] = ChannelOp.DELTA;
        view.setUint16(1, seq & 0xFFFF, true);
        view.setUint32(3, tick >>> 0, true);
        buf[7] = fragIdx & 0xFF;
        buf[8] = fragTotal & 0xFF;
        buf.set(chunk, DELTA_HEADER_LEN);
        return buf;
    }

    function encodeAck(seq, kind) {
        const buf = new Uint8Array(ACK_WIRE_LEN);
        buf[0] = ChannelOp.ACK;
        new DataView(buf.buffer).setUint16(1, seq & 0xFFFF, true);
        buf[3] = kind & 0xFF;
        return buf;
    }

    /**
     * Newest-wins filter plus fragment reassembler for the server delta channel.
     *
     * `push(frame)` returns `{ seq, tick, body }` exactly once per delta — when
     * its last chunk closes it — and `null` for a stale frame, an incomplete one,
     * or both. Unlike the Rust `FragmentAssembler`, which keeps a single slot and
     * so throws away tick 11's chunks when tick 10's arrive, this receiver lets
     * several deltas be in flight at once: with `ordered: false` the chunks of
     * tick 41 genuinely interleave with tick 40's, and closing 40 first should
     * not cost us 41's progress. Supersession still wins — once seq S is applied,
     * every buffered set that S overtakes can never be applied, so its chunks are
     * dropped immediately and later fragments of those sets count as stale.
     */
    class DeltaSequencer {
        constructor(nowFn) {
            this.nowFn = nowFn || (() => Date.now());
            this.newest = null;
            this.partials = new Map(); // seq -> { seq, tick, total, parts, received, at }
            this.staleCount = 0;
            this.assembledCount = 0;
            this.expiredCount = 0;
        }

        push(frame) {
            if (this.newest !== null &&
                (frame.seq === this.newest || !isNewerSeq(this.newest, frame.seq))) {
                this.staleCount++;
                return null;
            }
            const now = this.nowFn();

            if (frame.fragTotal === 1) {
                this._apply(frame.seq);
                return { seq: frame.seq, tick: frame.tick, body: frame.chunk };
            }

            let p = this.partials.get(frame.seq);
            // A changed frag_total for the same seq means the server restarted
            // the delta; the old geometry can never be completed honestly.
            if (p && p.total !== frame.fragTotal) {
                this.partials.delete(frame.seq);
                p = null;
            }
            if (!p) {
                if (this.partials.size >= MAX_INFLIGHT_DELTAS) this._evictStalest();
                p = {
                    seq: frame.seq, tick: frame.tick, total: frame.fragTotal,
                    parts: new Array(frame.fragTotal), received: 0, at: now,
                };
                this.partials.set(frame.seq, p);
            }
            p.at = now;
            // Duplicate chunk of a live set: place it once, count it once.
            if (p.parts[frame.fragIdx] === undefined) {
                p.parts[frame.fragIdx] = frame.chunk;
                p.received++;
            }
            if (p.received !== p.total) return null;

            let len = 0;
            for (const part of p.parts) len += part.length;
            const body = new Uint8Array(len);
            let off = 0;
            for (const part of p.parts) { body.set(part, off); off += part.length; }
            const applied = { seq: p.seq, tick: p.tick, body };
            this.partials.delete(frame.seq);
            this._apply(frame.seq);
            return applied;
        }

        /** Makes `seq` the authoritative delta and retires everything it supersedes. */
        _apply(seq) {
            this.newest = seq;
            for (const s of this.partials.keys()) {
                if (!isNewerSeq(seq, s)) this.partials.delete(s);
            }
            this.assembledCount++;
        }

        /**
         * Frees room for a new set by dropping the one that has progressed least
         * recently — with `p.at` refreshed by every chunk, that is the stalled one.
         */
        _evictStalest() {
            let victim = null;
            for (const [s, p] of this.partials) {
                if (victim === null || p.at < this.partials.get(victim).at) victim = s;
            }
            if (victim !== null) {
                this.partials.delete(victim);
                this.expiredCount++;
            }
        }

        /** Forgets half-received deltas that stopped making progress past the TTL. */
        cleanup() {
            const now = this.nowFn();
            let expired = false;
            for (const [s, p] of this.partials) {
                if (now - p.at > FRAGMENT_TTL_MS) {
                    this.partials.delete(s);
                    this.expiredCount++;
                    expired = true;
                }
            }
            return expired;
        }
    }

    function encodeInputFrame(input) {
        const buf = new Uint8Array(MESH_INPUT_LEN);
        const view = new DataView(buf.buffer);
        buf[0] = MeshOp.INPUT;
        view.setUint16(1, (input.seq || 0) >>> 0, true);
        view.setUint32(3, (input.flags || 0) >>> 0, true);
        view.setFloat32(7, input.aimYaw || 0, true);
        view.setFloat32(11, input.aimPitch || 0, true);
        view.setFloat32(15, input.moveX || 0, true);
        view.setFloat32(19, input.moveZ || 0, true);
        return buf;
    }

    /**
     * Packs at most MESH_MAX_ENTITIES records into one datagram-sized frame.
     * Overflow is capped rather than thrown: a 100-player room sends two frames.
     */
    function encodeStateFrame(tick, entities) {
        const n = Math.min(entities ? entities.length : 0, MESH_MAX_ENTITIES);
        if (n === 0) return null;
        const buf = new Uint8Array(MESH_STATE_HEADER_LEN + n * MESH_RECORD_LEN);
        const view = new DataView(buf.buffer);
        buf[0] = MeshOp.STATE;
        view.setUint16(1, (tick || 0) >>> 0, true);
        buf[3] = n;
        let at = MESH_STATE_HEADER_LEN;
        for (let i = 0; i < n; i++) {
            const e = entities[i];
            view.setUint16(at, (e.id || 0) >>> 0, true); at += 2;
            view.setInt16(at, quantizeLen(e.x || 0), true); at += 2;
            view.setInt16(at, quantizeLen(e.y || 0), true); at += 2;
            view.setInt16(at, quantizeLen(e.z || 0), true); at += 2;
            view.setInt16(at, quantizeAngle(e.yaw || 0), true); at += 2;
            view.setInt16(at, quantizeAngle(e.pitch || 0), true); at += 2;
            view.setInt16(at, quantizeLen(e.vx || 0), true); at += 2;
            view.setInt16(at, quantizeLen(e.vy || 0), true); at += 2;
            view.setInt16(at, quantizeLen(e.vz || 0), true); at += 2;
            buf[at] = (e.state || 0) & 0xFF; at += 1;
        }
        return buf;
    }

    function encodeEventFrame(event) {
        const buf = new Uint8Array(MESH_EVENT_LEN);
        const view = new DataView(buf.buffer);
        buf[0] = MeshOp.EVENT;
        view.setUint16(1, (event.seq || 0) >>> 0, true);
        buf[3] = (event.kind || 0) & 0xFF;
        view.setUint16(4, (event.from || 0) >>> 0, true);
        view.setUint16(6, event.target == null ? 0xFFFF : (event.target >>> 0), true);
        view.setUint16(8, (event.tick || 0) >>> 0, true);
        view.setFloat32(10, event.value || 0, true);
        return buf;
    }

    /** One datagram in, one frame out; null when the bytes are not a valid frame. */
    function decodeMeshFrame(bytes) {
        if (!bytes || bytes.length < 1) return null;
        const op = bytes[0];
        // Voice carries its own sequence and timestamp, and stays borrowed as-is.
        if (op === MeshOp.VOICE) return bytes.length >= 15 ? { op: MeshOp.VOICE, bytes } : null;

        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        if (op === MeshOp.INPUT) {
            if (bytes.length !== MESH_INPUT_LEN) return null;
            return {
                op, seq: view.getUint16(1, true), flags: view.getUint32(3, true),
                aimYaw: view.getFloat32(7, true), aimPitch: view.getFloat32(11, true),
                moveX: view.getFloat32(15, true), moveZ: view.getFloat32(19, true)
            };
        }
        if (op === MeshOp.STATE) {
            if (bytes.length < MESH_STATE_HEADER_LEN) return null;
            const count = bytes[3];
            if (count === 0 || bytes.length !== MESH_STATE_HEADER_LEN + count * MESH_RECORD_LEN) return null;
            const entities = new Array(count);
            let at = MESH_STATE_HEADER_LEN;
            for (let i = 0; i < count; i++) {
                entities[i] = {
                    id: view.getUint16(at, true),
                    x: dequantizeLen(view.getInt16(at + 2, true)),
                    y: dequantizeLen(view.getInt16(at + 4, true)),
                    z: dequantizeLen(view.getInt16(at + 6, true)),
                    yaw: dequantizeAngle(view.getInt16(at + 8, true)),
                    pitch: dequantizeAngle(view.getInt16(at + 10, true)),
                    vx: dequantizeLen(view.getInt16(at + 12, true)),
                    vy: dequantizeLen(view.getInt16(at + 14, true)),
                    vz: dequantizeLen(view.getInt16(at + 16, true)),
                    state: view.getUint8(at + 18)
                };
                at += MESH_RECORD_LEN;
            }
            return { op, tick: view.getUint16(1, true), count, entities };
        }
        if (op === MeshOp.EVENT) {
            if (bytes.length !== MESH_EVENT_LEN) return null;
            return {
                op, seq: view.getUint16(1, true), kind: view.getUint8(3),
                from: view.getUint16(4, true), target: view.getUint16(6, true),
                tick: view.getUint16(8, true), value: view.getFloat32(10, true)
            };
        }
        return null;
    }

    // =========================================================================
    // Schema state sync (own wire format, no msgpack / Colyseus dependency)
    // =========================================================================

    const SCHEMA_MAGIC = [0x4F, 0x58, 0x53, 0x32]; // "OXS2"
    const SCHEMA_TYPE_CODE = Object.freeze({
        u8: 1, u16: 2, u32: 3, i32: 4, f32: 5, f64: 6, bool: 7, i64: 8, i8: 9, i16: 10,
        string: 11, array: 12
    });
    const SCHEMA_FIELD_SIZE = Object.freeze({
        u8: 1, u16: 2, u32: 4, i32: 4, f32: 4, f64: 8, bool: 1, i64: 8, i8: 1, i16: 2,
        string: 0, array: 0
    });
    const SCHEMA_MAX_FIELDS = 32;
    const SCHEMA_FIELD_OPTIONAL = 1;
    const SCHEMA_FIELD_STATIC = 2;
    const SCHEMA_FIELD_TRANSIENT = 4;

    function schemaFnv1a(hash, text) {
        let h = hash >>> 0;
        const bytes = (typeof TextEncoder !== 'undefined')
            ? new TextEncoder().encode(text)
            : Uint8Array.from(text, c => c.charCodeAt(0));
        for (let i = 0; i < bytes.length; i++) {
            h = (h ^ bytes[i]) >>> 0;
            h = Math.imul(h, 16777619) >>> 0;
        }
        return h >>> 0;
    }

    /**
     * Client-side field list. On join the host sends a binary descriptor and
     * the SDK builds these automatically; hand-writing one is only needed
     * against a host that does not send a descriptor.
     *
     * ```js
     * const PlayerSchema = new SchemaDef("Player", 1)
     *   .u32("id").f64("x").f64("y").u32("flags");
     * room.useSchema(PlayerSchema); // optional
     * ```
     */
    class SchemaDef {
        constructor(name, version = 1) {
            this.name = name;
            this.version = version >>> 0;
            this.fields = [];
            this._byName = new Map();
            this.hash = schemaFnv1a(2166136261, `${name}|${this.version}`);
        }

        _add(name, type, flags = 0, element = null, nested = null) {
            if (this.fields.length >= SCHEMA_MAX_FIELDS) return this;
            if (this._byName.has(name)) throw new Error(`duplicate schema field '${name}'`);
            const code = SCHEMA_TYPE_CODE[type];
            if (!code) throw new Error(`unknown schema field type '${type}'`);
            const elementCode = element ? SCHEMA_TYPE_CODE[element] : 0;
            if (element && !elementCode) throw new Error(`unknown array element type '${element}'`);
            const field = {
                name,
                type,
                code,
                flags,
                optional: (flags & SCHEMA_FIELD_OPTIONAL) !== 0,
                fullOnly: (flags & (SCHEMA_FIELD_STATIC | SCHEMA_FIELD_TRANSIENT)) !== 0,
                element: element || null,
                elementCode,
                nested: nested || null,
                size: SCHEMA_FIELD_SIZE[type],
                offset: this.fields.reduce((n, f) => n + f.size, 0)
            };
            this.fields.push(field);
            this._byName.set(name, field);
            this._rehash();
            return this;
        }

        _rehash() {
            let hash = schemaFnv1a(2166136261, `${this.name}|${this.version}`);
            for (const field of this.fields) {
                let text = `|${field.name}:${field.code}`;
                if (field.elementCode) text += `.${field.elementCode}`;
                if (field.flags) text += `:${field.flags}`;
                if (field.nested) text += `:n${field.nested}`;
                hash = schemaFnv1a(hash, text);
            }
            this.hash = hash;
        }

        _last() {
            return this.fields[this.fields.length - 1] || null;
        }

        optional() {
            const field = this._last();
            if (field) {
                field.flags |= SCHEMA_FIELD_OPTIONAL;
                field.optional = true;
                this._rehash();
            }
            return this;
        }

        static() {
            const field = this._last();
            if (field) {
                field.flags |= SCHEMA_FIELD_STATIC;
                field.fullOnly = true;
                this._rehash();
            }
            return this;
        }

        staticField() { return this.static(); }

        transient() {
            const field = this._last();
            if (field) {
                field.flags |= SCHEMA_FIELD_TRANSIENT;
                field.fullOnly = true;
                this._rehash();
            }
            return this;
        }

        u8(name) { return this._add(name, 'u8'); }
        i8(name) { return this._add(name, 'i8'); }
        u16(name) { return this._add(name, 'u16'); }
        i16(name) { return this._add(name, 'i16'); }
        u32(name) { return this._add(name, 'u32'); }
        i32(name) { return this._add(name, 'i32'); }
        f32(name) { return this._add(name, 'f32'); }
        f64(name) { return this._add(name, 'f64'); }
        bool(name) { return this._add(name, 'bool'); }
        i64(name) { return this._add(name, 'i64'); }
        string(name) { return this._add(name, 'string'); }
        array(name, elementType) { return this._add(name, 'array', 0, elementType); }

        indexOf(name) {
            const f = this._byName.get(name);
            return f ? this.fields.indexOf(f) : -1;
        }
    }

    // ---- Schema descriptor ("OXSD") ---------------------------------------
    // Written by `cyg run` from schema.ts, pushed by the host right after
    // JOIN_ROOM. The client rebuilds the exact server field list from it, so a
    // project never ships a second schema file.

    const DESCRIPTOR_MAGIC = [0x4F, 0x58, 0x53, 0x44]; // "OXSD"
    const DESCRIPTOR_VERSION = 6;
    const SCHEMA_TYPE_NAME = Object.freeze([
        null, 'u8', 'u16', 'u32', 'i32', 'f32', 'f64', 'bool', 'i64', 'i8', 'i16',
        'string', 'array'
    ]);

    function isSchemaDescriptor(bytes) {
        return !!bytes && bytes.length >= 8
            && bytes[0] === DESCRIPTOR_MAGIC[0] && bytes[1] === DESCRIPTOR_MAGIC[1]
            && bytes[2] === DESCRIPTOR_MAGIC[2] && bytes[3] === DESCRIPTOR_MAGIC[3];
    }

    // The descriptor parser lived here (parseSchemaDescriptor, ~90 lines) and
    // the OXS2 section decoder below (~450 lines) — both now live ONLY in
    // cyg-core-rs (crates/cyg-core-rs), which every client SDK shares. cyg.js
    // reads the parsed contract back through the wasm ABI (cyg_schema_*) and
    // re-diffs the core state to drive its Colyseus-style callbacks.

    /**
     * One root collection — a Map of id -> entity with Colyseus-style hooks.
     * `room.state` is the first collection; `room.state.<name>` exposes the
     * others. `room.state` also carries root scalar fields as plain properties.
     */
    class SchemaMap extends Map {
        constructor(room, name = null, schema = null) {
            super();
            this._room = room;
            this._name = name;
            this._schema = schema;
            this._onAddHandlers = [];
            this._onRemoveHandlers = [];
            this._onChangeHandlers = [];
            this._ready = false;
        }
        onAdd(handler, immediate) {
            if (typeof handler !== 'function') return () => {};
            this._onAddHandlers.push(handler);
            if (immediate !== false && !this._room._isTriggering) {
                for (const [id, entity] of this) handler(entity, id);
            }
            return () => {
                const index = this._onAddHandlers.indexOf(handler);
                if (index >= 0) this._onAddHandlers.splice(index, 1);
            };
        }
        onRemove(handler) {
            if (typeof handler !== 'function') return () => {};
            this._onRemoveHandlers.push(handler);
            return () => {
                const index = this._onRemoveHandlers.indexOf(handler);
                if (index >= 0) this._onRemoveHandlers.splice(index, 1);
            };
        }
        onChange(handler) {
            if (typeof handler !== 'function') return () => {};
            this._onChangeHandlers.push(handler);
            return () => {
                const index = this._onChangeHandlers.indexOf(handler);
                if (index >= 0) this._onChangeHandlers.splice(index, 1);
            };
        }
        toJSON() {
            const out = {};
            for (const [id, entity] of this) out[id] = entity;
            return out;
        }
    }

    /**
     * Colyseus v5 callback API — `getStateCallbacks(room)` (alias:
     * `Callbacks.get(room)`).
     *
     * ```js
     * const callbacks = getStateCallbacks(room);
     * callbacks.onAdd("players", (player, id) => {
     *   callbacks.listen(player, "x", (current, previous) => {});
     * });
     * ```
     */
    class SchemaCallbacks {
        constructor(room) {
            this.room = room;
        }

        _collectionMap(value) {
            if (typeof value !== 'string') return null;
            const room = this.room;
            if (room._collectionMaps && room._collectionMaps.has(value)) {
                return room._collectionMaps.get(value);
            }
            const root = room._rootCollection;
            if (root ? value === root : value === 'players') return room.state;
            return null;
        }

        listen(instanceOrProperty, propertyOrHandler, handlerMaybe, immediate) {
            if (typeof instanceOrProperty === 'string') {
                const map = this._collectionMap(instanceOrProperty);
                if (map) {
                    return map.onChange(propertyOrHandler);
                }
                // Not a collection: a root scalar ("coinX"), routed through
                // the state entity's field callbacks — the apply path fires
                // root changes there.
                if (this.room.state && typeof propertyOrHandler === 'function') {
                    return this.listen(this.room.state, instanceOrProperty, propertyOrHandler, immediate);
                }
                return () => {};
            }
            const instance = instanceOrProperty;
            const property = propertyOrHandler;
            const handler = handlerMaybe;
            if (typeof handler !== 'function') return () => {};
            let meta = this.room._entityCallbacks.get(instance);
            if (!meta) {
                this.room._trackEntity(instance);
                meta = this.room._entityCallbacks.get(instance);
            }
            if (!meta) return () => {};
            // fields is a Map so root-scalar names like `__proto__` or
            // `constructor` can't collide with Object.prototype keys.
            let list = meta.fields.get(property);
            if (!list) { list = []; meta.fields.set(property, list); }
            list.push(handler);
            // Colyseus defaults `immediate` to true, but suppresses it while a
            // patch/add callback is already dispatching (a listener registered
            // from inside onAdd must not fire before the handler returns).
            if (immediate !== false && !this.room._isTriggering) {
                const current = instance[property];
                if (current !== undefined && current !== null) handler(current, undefined);
            }
            return () => {
                const index = list.indexOf(handler);
                if (index >= 0) list.splice(index, 1);
            };
        }

        onChange(instanceOrProperty, handler, immediate) {
            if (typeof instanceOrProperty === 'string') {
                const map = this._collectionMap(instanceOrProperty);
                if (!map || typeof handler !== 'function') {
                    return () => {};
                }
                return map.onChange(handler);
            }
            const meta = this.room._entityCallbacks.get(instanceOrProperty);
            if (!meta || typeof handler !== 'function') return () => {};
            meta.any.push(handler);
            if (immediate) handler();
            return () => {
                const index = meta.any.indexOf(handler);
                if (index >= 0) meta.any.splice(index, 1);
            };
        }

        onAdd(instanceOrProperty, handler, immediate) {
            const map = this._collectionMap(instanceOrProperty);
            if (!map) return () => {};
            return map.onAdd(handler, immediate !== false);
        }

        onRemove(instanceOrProperty, handler) {
            const map = this._collectionMap(instanceOrProperty);
            if (!map) return () => {};
            return map.onRemove(handler);
        }

        /** Mirror every property of `from` onto `to`, now and on every change. */
        bindTo(from, to, properties, immediate) {
            if (!from || !to) return () => {};
            const names = Array.isArray(properties) ? properties : Object.keys(from);
            const copyImmediately = immediate !== false && !this.room._isTriggering;
            const unsubscribers = [];
            for (let i = 0; i < names.length; i++) {
                const name = names[i];
                if (typeof from[name] === 'function') continue;
                if (copyImmediately && from[name] !== undefined) to[name] = from[name];
                // `listen` is registered with immediate=false; the initial copy
                // above is the single Colyseus `bindTo` immediate action.
                unsubscribers.push(this.listen(from, name, (current) => { to[name] = current; }, false));
            }
            return () => { for (let i = 0; i < unsubscribers.length; i++) unsubscribers[i](); };
        }
    }

    const Callbacks = Object.freeze({
        /** New Colyseus v5 API; accepts a room (or `{ state }`-shaped object). */
        get(roomOrDecoder) {
            return new SchemaCallbacks(roomOrDecoder && roomOrDecoder.room ? roomOrDecoder.room : roomOrDecoder);
        },
        /** Alias matching Colyseus v5's `getStateCallbacks(room)`. */
        getStateCallbacks(roomOrDecoder) {
            return Callbacks.get(roomOrDecoder);
        },
        getRawChanges() { return () => {}; }
    });

    class CYGRoom {
        constructor(endpoint, roomName, options = {}) {
            this.endpoint = endpoint.replace(/^http/, 'ws');
            this.roomName = roomName;
            this.options = options;
            this.isSpectator = !!options.spectator;
            this.ws = null;
            this.sessionId = null;
            // Numeric convenience mirror of `sessionId` (which is an i64).
            this.playerId = null;
            this.isOpen = false;
            // Transport counters (WebSocket + WebRTC data channels).
            this.bytesIn = 0;
            this.bytesOut = 0;
            this._webrtcEnabled = options.webrtc !== false;
            this.listeners = new Map();
            /** type -> [callback] for `room.onMessage(type, cb)` */
            this._messageHandlers = new Map();
            /** request/response (RPC-style): reqId -> {resolve, reject, timer} */
            this._pendingRequests = new Map();
            this._reqSeq = 0;
            // Attached WebRTC voice mesh, or null when voice is relay-only.
            this.voice = null;

            // Schema state: mirror of the server's `SchemaState`/`SchemaWorld`.
            this.schema = null;
            this.schemas = [];
            /** The descriptor's `Input` def — drives `room.sendInput({...})` packing. */
            this._inputSchema = null;
            this._inputLaneWarned = false;
            // cyg-core-rs wasm: ALL schema decode + input packing lives there.
            this._corePromise = loadCygCore().catch((err) => {
                this._coreError = (err && err.message) || String(err);
                return null;
            });
            this._core = null;
            this._coreHandle = 0;
            /** Schema payloads buffered while the wasm core loads. */
            this._pendingSchema = [];
            /** Primary collection name from the descriptor ("players", ...). */
            this._rootCollection = null;
            /** collection name -> SchemaMap. */
            this._collectionMaps = new Map();
            /** Ordered descriptor collections: `{ id, name, map, schema }`. */
            this._collections = [];
            /** Root scalar SchemaDef from a v2 descriptor, or null. */
            this._rootScalars = null;
            /** Root scalar values (`room.state.tick`). */
            this._rootValues = {};
            this.state = new SchemaMap(this);
            this._schemaReady = false;
            this._schemaError = null;
            this._onAddHandlers = [];
            this._onRemoveHandlers = [];
            this._onChangeHandlers = [];
            this._onCollectionChangeHandlers = [];
            this._onStateChangeHandlers = [];
            /** True while a patch/add dispatch is invoking user callbacks. */
            this._isTriggering = false;
            /** entity -> { fields: Map<name, [cb]>, any: [cb] } */
            this._entityCallbacks = new WeakMap();

            // Matchmaking / resume. `_seatSessionId` is the id the WebSocket
            // upgrade URL carries; `sessionId` is confirmed by the first delta.
            this.seatSessionId = options.sessionId || null;
            this._seatSessionId = this.seatSessionId;
            /** Colyseus parity: the room's public id (the seat's room name
             *  until the host assigns a distinct id). */
            this.roomId = options.roomId || this.roomName;
            this._leaveCode = null;
            this.reconnectionToken = options.reconnectionToken || null;
            this._client = options.client || null;
            this._autoReconnect = options.autoReconnect !== false;
            this._maxReconnectAttempts = options.maxReconnectAttempts ?? 5;
            /** Milliseconds to wait for the first full schema snapshot before
             *  giving up with `schemaerror`. 0 (the default) waits forever,
             *  matching the previous behaviour. A busy server can legitimately
             *  take seconds to seat and seed a room, so this is a watchdog for
             *  "never arrives", not a latency budget. */
            this._joinTimeoutMs = options.joinTimeout ?? 0;
            this._joinTimer = null;
            this._reconnectAttempts = 0;
            this._reconnectTimer = null;
            this._intentionalClose = false;

            // Replay recording state
            this.recording = false;
            this.recordStartTime = 0;
            this.recordedFrames = [];

            // QUIC-style protocol sequence counters
            this.inputSeq = 0;
            // Lossy-lane input redundancy: the same stamped frame is written
            // this many times on the unreliable DataChannel. Duplicate seqs
            // are dropped by the guest's newest-wins gate, so this only costs
            // a few bytes and never re-applies an old action. Reliable/WS
            // paths always send exactly one copy.
            this.inputRedundancy = Math.max(1, Math.min(4,
                Number.isFinite(options.inputRedundancy) ? options.inputRedundancy : 2));
            // Historical input replay: keep this many previous unacked frames
            // next to the current one and send them as one `__input_batch`
            // app message on the lossy lane. 0 disables batching.
            this.inputHistory = Math.max(0, Math.min(7,
                Number.isFinite(options.inputHistory) ? options.inputHistory : 2));
            this._inputHistory = [];

            // Phase D reconciliation: server-confirmed input seq / tick (from delta tail)
            this.ackedSeq = -1;
            this.serverTick = -1;

            // Phase D RTT echo: every `__input` frame carries a rotating u16
            // stamp trailer; the delta tail echoes it back and we pair the
            // echo with its send time (keyed by the stamp). `echoRtt` is the
            // RFC 793-style EMA (alpha 0.125), `echoJitter` the RFC 3550-style
            // mean deviation of the inter-arrival deltas. Falls through to the
            // ping-based `rtt`/`jitter` while no echo has landed yet.
            this._stampSeq = 0;
            this._stampTimes = new Map(); // stamp u16 -> sendMs
            this._stampOrder = [];
            this._echoRtt = -1;
            this._echoJitter = 0;
            this._lastEchoRtt = -1;
            this.echoSamples = 0;

            // Ping/RTT measurement (Lightyear-style PingManager lite)
            this._pingSeq = 0;
            this._pendingPings = new Map();
            this._pendingPingResolves = new Map();
            this._rttSamples = [];
            this.rtt = 0;
            this.jitter = 0;
            this._pingLoopTimer = null;
            // PingManager full port: rolling window stats + in-flight tracking.
            this._pingWindow = [];
            this._pingWindowMax = 30;
            this._oldestPendingSeq = null;
            this._oldestPendingAt = 0;
            this.rttMin = 0;
            this.rttMax = 0;

            // Jitter buffer for smooth delta delivery (Lightyear-style margin)
            this.jitterBufferEnabled = false;
            this.jitterDelayMs = 80;
            this.autoJitterBuffer = true;
            this._jitterQueue = [];
            this._jitterTimer = null;

            // TimeSync: robust regression of arrival time against server tick.
            this.timeSync = new CYGTimeSync();
            this.tickRate = 128;
            this.clockOffsetMs = 0;
            this.tickOriginMs = -1;
            this.timeSync.setTickRate(this.tickRate);

            // SeqFilter: newest-wins gate on the authoritative delta tick.
            this.lastAppliedTick = -1;
            /** Newest volatile-only patch tick already applied. Its stream is
             *  lossy and unordered, so older patches are discarded instead of
             *  letting a late position frame overwrite a newer one. */
            this.lastVolatileTick = -1;
            /** tick -> partial 0xA7 volatile fragments. Small and bounded:
             *  only one or two tick sets can be in flight on a lossy lane. */
            this._volatileFragments = new Map();

            // Interpolation timeline engine (game-agnostic pose buffer).
            this._poseBuffers = new Map();
            this._poseScratch = new Map();
            this.poseBufferMax = 64;
            this.extrapMaxAgeMs = 100;
            // WebRTC Client-Server DataChannels (str0m UDP star topology)
            this.serverPc = null;
            this.serverChUnreliable = null;
            this.serverChReliable = null;
            this.serverRtcActive = false;
            this.serverRtcConnecting = false;

            // Server delta channel: newest-wins filter + fragment reassembler.
            // `deltaTick` is the authoritative tick the body describes, which is
            // what interpolation reads; the counters mirror the sequencer's
            // ledger for the HUD (see `deltaStats()`).
            this.deltas = new DeltaSequencer();
            this.deltaSeq = 0;
            this.deltaTick = 0;
            this.staleDeltas = 0;
            this.reassembledDeltas = 0;
            this.expiredDeltas = 0;
            this._fragmentTimer = null;

            this.timeline = null;
            this.interpolationEnabled = false;
            this._deltaDecoder = null;
            this.lastDeltaArrivalMs = -1;

            // Client prediction (see CYGPredictDriver). The web SDK does not
            // instantiate the guest WASM itself — an ECS-template page or a
            // future runtime embed hands the instance to attachPredict();
            // until then `room.predict` is a feature-detected no-op stub.
            this._predictInstance = options.predictInstance || null;
            this._predict = null;
        }

        /**
         * Client prediction surface. Feature-detected: `room.predict`
         * returns a CYGPredictDriver whose `available` is true only after a
         * guest instance with the full predict export set was attached —
         * until then every call is a no-op, so games can call
         * `room.predict.tick(...)` unconditionally.
         */
        get predict() {
            if (!this._predict) {
                this._predict = new CYGPredictDriver(this, this._predictInstance);
            }
            return this._predict;
        }

        /** Hand the guest WASM instance (with cyg_predict_tick et al.) to
         *  the driver. Returns true when the export set is complete. */
        attachPredict(instance, opts = {}) {
            this._predictInstance = instance;
            if (!this._predict) this._predict = new CYGPredictDriver(this);
            return this._predict.attachPredict(instance, opts);
        }

        /** Confirms that an event arrived — reliable channel first, WS as fallback. */
        _ackEvent(seq) {
            const frame = encodeAck(seq, AckKind.EVENT);
            const ch = this.serverChReliable;
            if (this.serverRtcActive && ch && ch.readyState === 'open') {
                try { ch.send(frame); return; } catch (e) { /* closing */ }
            }
            if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                try { this.ws.send(frame); } catch (e) { /* closing */ }
            }
        }

        /**
         * App-level RTT measurement: [18][seq u32 LE] echoed by the server.
         * Resolves with RTT in ms (-1 on timeout).
         */
        ping(callback) {
            const promise = this._pingImpl();
            if (typeof callback === 'function') promise.then((ms) => callback(ms));
            return promise;
        }

        _pingImpl() {
            return new Promise((resolve) => {
                const chReliable = this.serverRtcActive ? this.serverChReliable : null;
                const hasRttLane = (chReliable && chReliable.readyState === 'open') ||
                    (this.ws && this.ws.readyState === 1);
                if (!hasRttLane) return resolve(-1);
                const seq = (this._pingSeq = (this._pingSeq + 1) & 0xFFFFFFFF) >>> 0;
                const sentAt = performance.now();
                this._pendingPings.set(seq, sentAt);
                this._pendingPingResolves.set(seq, resolve);
                if (this._oldestPendingSeq == null || !this._pendingPings.has(this._oldestPendingSeq)) {
                    this._oldestPendingSeq = seq;
                    this._oldestPendingAt = sentAt;
                }
                const buf = new Uint8Array(5);
                buf[0] = 18;
                new DataView(buf.buffer).setUint32(1, seq, true);
                // RTT probe rides the reliable DataChannel when RTC is active —
                // the echo handler is transport-agnostic, so nothing else moves.
                if (chReliable && chReliable.readyState === 'open') {
                    try { chReliable.send(buf); } catch (e) { return resolve(-1); }
                } else {
                    this.ws.send(buf);
                }
                setTimeout(() => {
                    if (this._pendingPings.has(seq)) {
                        this._pendingPings.delete(seq);
                        this._pendingPingResolves.delete(seq);
                        resolve(-1);
                    }
                }, 2000);
            });
        }

        /** Continuously measure RTT/jitter. Results in room.rtt / room.jitter. */
        startPingLoop(intervalMs = 1000) {
            this._pingIntervalMs = intervalMs;
            this.stopPingLoop();
            this._pingLoopTimer = setInterval(() => {
                // Skip firing while a sample is still in flight (slow links / overlap).
                if (this._pendingPings.size === 0) { this.ping(); return; }
                if (performance.now() - this._oldestPendingAt > 2000) this._advanceOldestPending();
                if (this._pendingPings.size === 0) this.ping();
            }, intervalMs);
            this.ping();
        }

        stopPingLoop() {
            if (this._pingLoopTimer) { clearInterval(this._pingLoopTimer); this._pingLoopTimer = null; }
        }

        /** Drop a timed-out sample so the in-flight guard cannot stall the loop. */
        _advanceOldestPending() {
            this._pendingPings.delete(this._oldestPendingSeq);
            this._pendingPingResolves.delete(this._oldestPendingSeq);
            this._oldestPendingSeq = null;
            for (const s of this._pendingPings.keys()) {
                if (this._oldestPendingSeq == null) {
                    this._oldestPendingSeq = s;
                    this._oldestPendingAt = this._pendingPings.get(s);
                } else if (this._pendingPings.get(s) < this._oldestPendingAt) {
                    this._oldestPendingSeq = s;
                    this._oldestPendingAt = this._pendingPings.get(s);
                }
            }
        }

        /**
         * PingManager rolling-window stats on every [18] echo: rtt is an EWMA,
         * jitter the mean absolute deviation of the last 30 samples, plus min/max.
         * Also feeds the adaptive jitter-buffer delay.
         */
        _onPingEcho(seq, sentAt, now) {
            const rtt = now - sentAt;
            const w = this._pingWindow;
            w.push(rtt);
            if (w.length > this._pingWindowMax) w.shift();
            let sum = 0, mn = Infinity, mx = -Infinity;
            for (const s of w) { sum += s; if (s < mn) mn = s; if (s > mx) mx = s; }
            const avg = sum / w.length;
            let dev = 0;
            for (const s of w) dev += Math.abs(s - avg);
            dev /= w.length;
            this.rttMin = Math.round(mn);
            this.rttMax = Math.round(mx);
            this.jitter = Math.round(dev);
            const a = this.rtt === 0 ? 1 : 0.3;
            this.rtt = Math.round(this.rtt === 0 ? avg : this.rtt * (1 - a) + avg * a);
            if (this.autoJitterBuffer) {
                this.jitterDelayMs = Math.max(20, Math.min(250, Math.round(2 * this.jitter + 8)));
            }
            const resolve = this._pendingPingResolves.get(seq);
            if (resolve) { this._pendingPingResolves.delete(seq); resolve(rtt); }
        }

        /**
         * Jitter buffer: deltas are delivered `delayMs` after arrival in FIFO
         * order, absorbing network bursts (Lightyear-style jitter margin).
         */
        enableJitterBuffer(delayMs) {
            this.jitterBufferEnabled = true;
            // An explicit number is a manual override that pins the delay and
            // turns the adaptive tuner off; omitting it leaves auto-tuning active.
            if (delayMs != null) {
                this.jitterDelayMs = delayMs;
                this.autoJitterBuffer = false;
            } else if (!this.autoJitterBuffer) {
                this.jitterDelayMs = this.jitterDelayMs || 80;
            }
            if (!this._jitterTimer) {
                this._jitterTimer = setInterval(() => {
                    if (!this.jitterBufferEnabled) return;
                    const now = performance.now();
                    while (this._jitterQueue.length && this._jitterQueue[0].deliverAt <= now) {
                        const item = this._jitterQueue.shift();
                        this.lastDeltaArrivalMs = item.arrivalMs ?? item.deliverAt;
                        this.emit('delta', item.payload, item.ackedSeq, item.serverTick);
                    }
                }, 8);
            }
        }

        disableJitterBuffer() {
            this.jitterBufferEnabled = false;
            const q = this._jitterQueue;
            this._jitterQueue = [];
            for (const item of q) this.emit('delta', item.payload, item.ackedSeq, item.serverTick);
        }

        /**
         * Interpolation timeline: remote entities are rendered `interpTicks` plus
         * a jitter margin *behind* the newest server tick, blended by server tick
         * rather than by arrival time. Supersedes `enableJitterBuffer()` — using
         * both double-counts the delay.
         *
         * `decoder(bytes) => ({ id: Float64Array })` turns a game delta into
         * numeric snapshots; with one set, every delta on either transport is
         * pushed automatically, so the app only calls `sampleInterpolation()`
         * per frame.
         */
        enableInterpolation(options = {}, decoder = null) {
            this.timeline = new CYGInterpolationTimeline(options);
            this.interpolationEnabled = true;
            if (decoder) this._deltaDecoder = decoder;
            return this.timeline;
        }

        disableInterpolation() {
            this.interpolationEnabled = false;
            this.timeline = null;
            this._deltaDecoder = null;
        }

        /** Record one authoritative snapshot (arrival time defaults to the delta's). */
        pushInterpolation(tick, entities, arrivalMs) {
            if (!this.timeline) return null;
            this.timeline.pushSnapshot(tick, entities, arrivalMs ?? this.lastDeltaArrivalMs);
            return this.timeline;
        }

        /** Advance the render clock and return the blended snapshot, or null. */
        sampleInterpolation(nowMs) {
            return this.timeline ? this.timeline.sample(nowMs) : null;
        }

        interpolationStats() {
            return this.timeline ? this.timeline.stats() : null;
        }

        /**
         * Clock offset / server timeline (TimeSync). `serverNow()` answers "what
         * wall-clock is it on the server right now, in client performance.now()
         * epoch"; `tickToTimeMs` / `timeToTick` map between the two through the
         * fitted line. All updated lazily on each delta, zero extra traffic.
         */
        serverNow() {
            const now = performance.now();
            return this.timeSync.synced ? now + this.clockOffsetMs : now;
        }

        tickToTimeMs(tick) {
            return this.timeSync.tickToTimeMs(tick);
        }

        timeToTick(t) {
            return this.timeSync.timeToTick(t);
        }

        /**
         * Record one pose snapshot for `id` at `serverTick` into a ≤64-entry ring,
         * reusing the stored object to avoid GC churn. Older ticks are dropped once
         * the ring is full and a same-tick retransmit simply overwrites in place.
         */
        bufferState(id, serverTick, poseObj) {
            if (!Number.isFinite(serverTick)) return;
            let buf = this._poseBuffers.get(id);
            if (!buf) { buf = []; this._poseBuffers.set(id, buf); }
            const last = buf.length ? buf[buf.length - 1] : null;
            if (last && serverTick < last.tick) return; // superseded
            if (last && last.tick === serverTick) {
                copyPose(last.pose, poseObj);
                return;
            }
            if (buf.length >= this.poseBufferMax) buf.shift();
            const slot = { tick: serverTick, pose: {} };
            buf.push(slot);
            copyPose(slot.pose, poseObj);
        }

        /**
         * Interpolated pose for `id` at `renderTimeMs`, or null before any snapshot.
         * Render time is converted to a fractional tick through the fitted line, the
         * bracketing snapshots are found, and every numeric field is lerped (mixed
         * fields: numbers lerp, non-numbers keep the earlier snapshot's value). When
         * render time is ahead of the newest snapshot it extrapolates along the last
         * per-tick delta, clamped to 100 ms of age. The returned object is a per-id
         * scratch that is reused on the next call — copy it if you must retain it.
         */
        sampleState(id, renderTimeMs) {
            const buf = this._poseBuffers.get(id);
            if (!buf || buf.length === 0) return null;
            const target = this.timeSync.timeToTick(renderTimeMs);
            const newest = buf[buf.length - 1];
            if (target >= newest.tick) {
                const out = this._scratchFor(id);
                let extraTicks = target - newest.tick;
                if (buf.length >= 2 && extraTicks > 0) {
                    const prev = buf[buf.length - 2];
                    const gap = newest.tick - prev.tick;
                    if (gap > 0) {
                        const maxExtraTicks = this.extrapMaxAgeMs / this.timeSync.msPerTick;
                        if (extraTicks > maxExtraTicks) extraTicks = maxExtraTicks;
                        this._blendPose(prev.pose, newest.pose, 1 + extraTicks / gap, out);
                        return out;
                    }
                }
                copyPose(out, newest.pose);
                return out;
            }
            if (target <= buf[0].tick) {
                const out = this._scratchFor(id);
                copyPose(out, buf[0].pose);
                return out;
            }
            let i = buf.length - 1;
            while (i > 0 && buf[i].tick > target) i--;
            const a = buf[i], b = buf[i + 1];
            let alpha = (target - a.tick) / (b.tick - a.tick);
            if (alpha < 0) alpha = 0; else if (alpha > 1) alpha = 1;
            const out = this._scratchFor(id);
            this._blendPose(a.pose, b.pose, alpha, out);
            return out;
        }

        _scratchFor(id) {
            let s = this._poseScratch.get(id);
            if (!s) { s = {}; this._poseScratch.set(id, s); }
            return s;
        }

        /** Blend two poses into `out`: numbers lerp by alpha, non-numbers keep the
         *  earlier snapshot `a` (lower tick); keys unique to either side survive. */
        _blendPose(a, b, alpha, out) {
            for (const k in b) out[k] = b[k];
            for (const k in a) {
                const va = a[k], vb = b[k];
                out[k] = (typeof va === 'number' && typeof vb === 'number')
                    ? va + (vb - va) * alpha
                    : va;
            }
            return out;
        }

        /** Drop every buffered pose and scratch state for one entity. */
        forgetEntity(id) {
            this._poseBuffers.delete(id);
            this._poseScratch.delete(id);
        }

        /**
         * Connect to the room. DataChannel-first: a seat from matchmake
         * negotiates `/ws/signal` + the server's UDP DataChannels directly —
         * the game-port WebSocket only exists on hosts that still run one, so
         * it stays as the fallback, not the path.
         */
        connect() {
            const canRtc = this._webrtcEnabled !== false &&
                typeof RTCPeerConnection !== 'undefined' &&
                !!this._client &&
                !!(this.reconnectionToken || this._seatSessionId);
            if (canRtc) return this._connectViaSignal();
            return this._connectWebSocket();
        }

        /**
         * DataChannel join: dial the control plane's `/ws/signal` with the
         * seat token, answer the SDP dance over the one JSON protocol the
         * hub speaks (`offer`/`candidate` up, `ice`/`answer`/`candidate`/
         * `error` down), and run the room on the negotiated DataChannels.
         * The schema descriptor arrives on the reliable lane right after the
         * channel opens; the first delta's `your_id` confirms the session.
         */
        _connectViaSignal() {
            return new Promise((resolve, reject) => {
                this._connectResolve = resolve;
                this._connectReject = reject;
                let url;
                try {
                    const base = new URL(this._client._httpBase());
                    const proto = base.protocol === 'https:' ? 'wss:' : 'ws:';
                    const token = this.reconnectionToken || this._seatSessionId || '';
                    url = `${proto}//${base.host}/ws/signal?seat=${encodeURIComponent(token)}`;
                } catch (e) {
                    return reject(e);
                }
                let ws;
                try {
                    ws = new WebSocket(url);
                } catch (e) {
                    return reject(e);
                }
                this.signalWs = ws;
                const fail = (err) => {
                    if (!this.isOpen) {
                        const r = this._connectReject;
                        this._connectResolve = null;
                        this._connectReject = null;
                        if (this._signalJoinTimer) {
                            clearTimeout(this._signalJoinTimer);
                            this._signalJoinTimer = null;
                        }
                        if (r) r(err);
                        this.emit('error', err);
                    }
                };
                // Join watchdog: the signaling hub can accept the socket and
                // then go silent (no ICE config, no answer) — reject instead of
                // hanging forever, mirroring the WS _armJoinWatchdog.
                const joinTimeoutMs = this._joinTimeoutMs || 15000;
                this._signalJoinTimer = setTimeout(() => {
                    this._signalJoinTimer = null;
                    if (this.isOpen) return;
                    try { ws.close(4000, 'join timeout'); } catch (_) { /* already closing */ }
                    fail(new Error(`[CYG SDK] signaling join timed out after ${joinTimeoutMs} ms (no data channel)`));
                }, joinTimeoutMs);
                if (this._signalJoinTimer && typeof this._signalJoinTimer.unref === 'function') {
                    this._signalJoinTimer.unref();
                }
                ws.onerror = () => fail(new Error('[CYG SDK] signaling socket failed'));
                ws.onclose = () => {
                    if (!this.isOpen) {
                        fail(new Error('[CYG SDK] signaling closed before the data channel opened'));
                    }
                };
                ws.onmessage = (ev) => {
                    let msg = null;
                    try { msg = JSON.parse(ev.data); } catch (_) { return; }
                    if (msg.type === 'ice') {
                        this._startSignalPeer(msg.config).catch(fail);
                    } else if (msg.type === 'answer') {
                        if (this.serverPc && this.serverPc.signalingState !== 'stable') {
                            this.serverPc.setRemoteDescription({ type: 'answer', sdp: msg.sdp }).catch(fail);
                        }
                    } else if (msg.type === 'candidate') {
                        if (this.serverPc) {
                            this.serverPc.addIceCandidate({
                                candidate: msg.candidate,
                                sdpMid: msg.sdpMid == null ? null : msg.sdpMid,
                                sdpMLineIndex: msg.sdpIndex == null ? null : msg.sdpIndex
                            }).catch(() => { /* a late candidate from a dead pc is harmless */ });
                        }
                    } else if (msg.type === 'error') {
                        fail(new Error(`[CYG SDK] signaling refused: ${msg.message}`));
                    }
                };
            });
        }

        /** Build the peer connection and send the offer once ICE config lands. */
        async _startSignalPeer(iceConfig) {
            if (this.serverPc) return;
            const isLocal = (typeof location !== 'undefined') &&
                (location.hostname === 'localhost' || location.hostname === '127.0.0.1' ||
                    location.hostname.startsWith('192.168.') || location.hostname.startsWith('10.') ||
                    location.hostname === '');
            const config = (iceConfig && Array.isArray(iceConfig.iceServers) && iceConfig.iceServers.length)
                ? iceConfig
                : (isLocal ? { iceServers: [] } : { iceServers: [{ urls: 'stun:stun.l.google.com:19302' }] });
            const pc = new RTCPeerConnection(config);
            this.serverPc = pc;
            this._wireServerChannels(pc);

            pc.onicecandidate = (e) => {
                if (e.candidate && e.candidate.candidate &&
                    this.signalWs && this.signalWs.readyState === 1) {
                    this.signalWs.send(JSON.stringify({
                        type: 'candidate',
                        candidate: e.candidate.candidate,
                        sdpMid: e.candidate.sdpMid,
                        sdpMLineIndex: e.candidate.sdpMLineIndex
                    }));
                }
            };

            pc.onconnectionstatechange = () => {
                if (pc.connectionState === 'failed') {
                    this._onServerLaneLost('rtc-failed');
                } else if (pc.connectionState === 'disconnected') {
                    this.serverRtcActive = false;
                }
            };

            const offer = await pc.createOffer();
            await pc.setLocalDescription(offer);
            this.signalWs.send(JSON.stringify({ type: 'offer', sdp: pc.localDescription.sdp }));
        }

        /**
         * A server lane (DataChannel or the whole peer connection) died. On the
         * WebSocket transport RTC is only an upgrade — the WS keeps the room
         * alive. On the DataChannel path a lost lane means the room is gone, so
         * mirror the WS onclose behavior (cleanup + notify + auto-reconnect).
         */
        _onServerLaneLost(lane) {
            this.serverRtcActive = false;
            if (this._intentionalClose || !this.isOpen) return;
            if (this.ws && this.ws.readyState === WebSocket.OPEN) return;
            const ev = { code: 1006, reason: `server ${lane} lane lost` };
            this.isOpen = false;
            if (this.serverPc) {
                try { this.serverPc.close(); } catch (_) {}
                this.serverPc = null;
            }
            if (this.signalWs) {
                try { this.signalWs.close(); } catch (_) {}
                this.signalWs = null;
            }
            if (this.voice) this.voice.dispose();
            this.stopPingLoop();
            this._pendingPings.clear();
            this._pendingPingResolves.clear();
            this._oldestPendingSeq = null;
            if (this._jitterTimer) { clearInterval(this._jitterTimer); this._jitterTimer = null; }
            if (this._fragmentTimer) { clearInterval(this._fragmentTimer); this._fragmentTimer = null; }
            this.emit('close', ev);
            this._maybeAutoReconnect(ev);
        }

        /** The two contract channels: `game-unreliable` (newest-wins deltas)
         *  and `game-reliable` (ordered events + the schema descriptor). */
        _wireServerChannels(pc) {
            // 1. game-unreliable: unordered, 0 retransmissions for 50-60Hz movement & state deltas over UDP
            const chUnreliable = pc.createDataChannel('game-unreliable', {
                ordered: false,
                maxRetransmits: 0
            });
            chUnreliable.binaryType = 'arraybuffer';

            // 2. game-reliable: ordered, reliable SCTP channel for gameplay events (fire, hit, score, kill)
            const chReliable = pc.createDataChannel('game-reliable', {
                ordered: true
            });
            chReliable.binaryType = 'arraybuffer';

            this.serverChUnreliable = chUnreliable;
            this.serverChReliable = chReliable;

            const onBytes = (ev) => {
                let bytes = null;
                if (ev.data instanceof ArrayBuffer) {
                    bytes = new Uint8Array(ev.data);
                } else if (ev.data && ev.data.buffer) {
                    bytes = new Uint8Array(ev.data.buffer, ev.data.byteOffset || 0, ev.data.byteLength || ev.data.length);
                }
                if (bytes) {
                    this._handleInboundBinary(bytes);
                }
            };

            chUnreliable.onopen = () => {
                this.serverRtcActive = true;
                this.serverRtcConnecting = false;
                if (!this.isOpen && this.isSpectator) {
                    try { chUnreliable.send(new Uint8Array([0xFF, 0x53, 0x50, 0x45, 0x43])); } catch (e) { /* closing */ }
                }
                this.emit('server-webrtc-open');
            };

            chReliable.onopen = () => {
                this.emit('server-webrtc-reliable-open');
                if (!this.isOpen) {
                    const isReconnect = this._hasConnected && this._reconnectAttempts > 0;
                    this.isOpen = true;
                    this._hasConnected = true;
                    this._reconnectAttempts = 0;
                    this.emit('open');
                    if (isReconnect) this.emit('reconnect');
                    const resolve = this._connectResolve;
                    this._connectResolve = null;
                    this._connectReject = null;
                    if (this._signalJoinTimer) {
                        clearTimeout(this._signalJoinTimer);
                        this._signalJoinTimer = null;
                    }
                    if (resolve) resolve(this);
                }
            };

            chUnreliable.onclose = () => {
                this._onServerLaneLost('unreliable');
            };
            chReliable.onclose = () => {
                // Either lane dying means the peer connection is going down.
                this._onServerLaneLost('reliable');
            };

            chUnreliable.onmessage = onBytes;
            chReliable.onmessage = onBytes;
        }

        _connectWebSocket() {
            return new Promise((resolve, reject) => {
                let cleanUrl = this.endpoint.endsWith('/') ? this.endpoint : `${this.endpoint}/`;
                let wsUrl = `${cleanUrl}${this.roomName}`;
                const query = [];
                const seatSession = this._seatSessionId || this.sessionId;
                if (seatSession) query.push(`sessionId=${encodeURIComponent(seatSession)}`);
                if (this.isSpectator) query.push('spectator=1');
                // Join options travel as percent-encoded JSON so the server's
                // `onAuth(clientId, options)` sees exactly what the game passed.
                const joinOptions = {};
                let hasOptions = false;
                for (const k of Object.keys(this.options || {})) {
                    if (k === 'client' || k === 'spectator' || k === 'sessionId' ||
                        k === 'reconnectionToken' || k === 'autoReconnect' ||
                        k === 'maxReconnectAttempts' || k === 'webrtc') continue;
                    joinOptions[k] = this.options[k];
                    hasOptions = true;
                }
                if (hasOptions) {
                    try {
                        query.push(`options=${encodeURIComponent(JSON.stringify(joinOptions))}`);
                    } catch (e) { /* non-JSON options are auth-visible only if serializable */ }
                }
                if (query.length) wsUrl += `?${query.join('&')}`;

                try {
                    this.ws = new WebSocket(wsUrl);
                    // `binaryType` must be set before the first frame lands and
                    // the message handler attached immediately: the host writes
                    // the join reply, the schema descriptor and the first full
                    // snapshot in the same burst as the 101, so frames can
                    // arrive before the handlers further down are installed and
                    // would otherwise be dropped (client 0 wins the race, every
                    // later joiner loses it).
                    this.ws.binaryType = 'arraybuffer';
                    this.ws.onmessage = (ev) => {
                        const bytes = toBinaryBytes(ev.data);
                        if (bytes) this._handleInboundBinary(bytes);
                    };
                    this._armJoinWatchdog();
                } catch (e) {
                    return reject(e);
                }

                this.ws.onopen = () => {
                    this.isOpen = true;
                    this._hasConnected = true;
                    this._reconnectAttempts = 0;
                    if (this.isSpectator) {
                        this.ws.send(new Uint8Array([0xFF, 0x53, 0x50, 0x45, 0x43]));
                    }
                    this.emit('open');
                    if (this._webrtcEnabled) this.connectServerWebRtc();
                    resolve(this);
                };

                this.ws.onerror = (err) => {
                    this.emit('error', err);
                    if (!this.isOpen) reject(err);
                };

                this.ws.onclose = (ev) => {
                    this.isOpen = false;
                    this.serverRtcActive = false;
                    if (this.serverPc) {
                        try { this.serverPc.close(); } catch (_) {}
                        this.serverPc = null;
                    }
                    if (this.voice) this.voice.dispose();
                    this.stopPingLoop();
                    this._pendingPings.clear();
                    this._pendingPingResolves.clear();
                    this._oldestPendingSeq = null;
                    if (this._jitterTimer) { clearInterval(this._jitterTimer); this._jitterTimer = null; }
                    if (this._fragmentTimer) { clearInterval(this._fragmentTimer); this._fragmentTimer = null; }
                    this.emit('close', ev);
                    this._maybeAutoReconnect(ev);
                };

            });
        }

        /**
         * Initialize direct WebRTC DataChannels with Rust Game Server over UDP (str0m)
         */
        connectServerWebRtc() {
            if (typeof RTCPeerConnection === 'undefined' || this.serverRtcConnecting || this.serverRtcActive) {
                return;
            }
            this.serverRtcConnecting = true;

            const isLocal = (typeof location !== 'undefined') &&
                (location.hostname === 'localhost' || location.hostname === '127.0.0.1' || location.hostname.startsWith('192.168.') || location.hostname.startsWith('10.') || location.hostname === '');
            const config = isLocal ? { iceServers: [] } : {
                iceServers: [
                    { urls: 'stun:stun.l.google.com:19302' }
                ]
            };

            try {
                const pc = new RTCPeerConnection(config);
                this.serverPc = pc;
                this._wireServerChannels(pc);

                pc.onicecandidate = (e) => {
                    if (e.candidate && e.candidate.candidate) {
                        this.sendSignal(SignalKind.ICE, 0, {
                            candidate: e.candidate.candidate,
                            sdpMid: e.candidate.sdpMid,
                            sdpMLineIndex: e.candidate.sdpMLineIndex
                        });
                    }
                };

                pc.onconnectionstatechange = () => {
                    console.log(`[CYG SDK] ⚡ Server WebRTC connection state: ${pc.connectionState}`);
                    if (pc.connectionState === 'connected') {
                        this.serverRtcActive = true;
                        this.serverRtcConnecting = false;
                        this.emit('server-webrtc-open');
                    } else if (pc.connectionState === 'failed') {
                        this._onServerLaneLost('rtc-failed');
                    } else if (pc.connectionState === 'disconnected') {
                        this.serverRtcActive = false;
                    }
                };

                pc.createOffer().then(offer => {
                    return pc.setLocalDescription(offer);
                }).then(() => {
                    this.sendSignal(SignalKind.OFFER, 0, { sdp: pc.localDescription.sdp });
                    console.log('[CYG SDK] ⚡ WebRTC SDP Offer sent to Server');
                }).catch(err => {
                    console.warn('[CYG SDK] WebRTC server createOffer error:', err);
                    this.serverRtcConnecting = false;
                });
            } catch (err) {
                console.warn('[CYG SDK] WebRTC server setup error:', err);
                this.serverRtcConnecting = false;
            }
        }

        _handleInboundBinary(bytes) {
            this.bytesIn += bytes.byteLength;
            if (!bytes || bytes.length === 0) return;

            // Schema descriptor ("OXSD"): sent once, right after JOIN_ROOM.
            // This is the whole client-side schema contract — no project file.
            if (isSchemaDescriptor(bytes)) {
                this._attachDescriptor(bytes);
                return;
            }

            // A volatile frame larger than one datagram is split locally as
            // [0xA7][tick u32][idx u8][total u8][chunk]. The assembled bytes
            // are the original [0xA6][tick] wrapped delta.
            if (bytes[0] === 0xA7 && bytes.length >= 7) {
                const view = new DataView(
                    bytes.buffer, bytes.byteOffset + 1, 4
                );
                const tick = view.getUint32(0, true);
                const index = bytes[5];
                const total = bytes[6];
                if (total === 0 || index >= total) return true;
                let set = this._volatileFragments.get(tick);
                if (!set || set.total !== total) {
                    set = { total, parts: new Array(total), received: 0 };
                    this._volatileFragments.set(tick, set);
                }
                if (set.parts[index] === undefined) {
                    set.parts[index] = bytes.slice(7);
                    set.received++;
                }
                if (set.received === set.total) {
                    let len = 0;
                    for (const part of set.parts) len += part.length;
                    const assembled = new Uint8Array(len);
                    let off = 0;
                    for (const part of set.parts) {
                        assembled.set(part, off);
                        off += part.length;
                    }
                    this._volatileFragments.delete(tick);
                    if (assembled[0] === 0xA6 && tick > this.lastVolatileTick) {
                        this.lastVolatileTick = tick;
                        this._processDeltaPayload(assembled.subarray(5), true);
                    }
                }
                while (this._volatileFragments.size > 8) {
                    this._volatileFragments.delete(
                        this._volatileFragments.keys().next().value
                    );
                }
                return true;
            }

            // Volatile-only state stream: [0xA6][tick u32] + a normal wrapped
            // delta body. It carries `.unreliable()` fields and never advances
            // the reliable tick ledger.
            if (bytes[0] === 0xA6 && bytes.length >= 5) {
                const tick = new DataView(
                    bytes.buffer, bytes.byteOffset + 1, 4
                ).getUint32(0, true);
                if (tick > this.lastVolatileTick) {
                    this.lastVolatileTick = tick;
                    this._processDeltaPayload(bytes.subarray(5), true);
                }
                return;
            }

            // Stamped server delta on `game-unreliable`:
            // [0x04][seq u16][tick u32][frag_idx u8][frag_total u8][chunk]
            if (bytes[0] === 0x04 && bytes.length >= 9) {
                if (this._handleDeltaFrame(bytes)) return;
            }

            // WebRTC signaling from the server: opcode 0x11 + 'W', or a
            // bare 'W' frame on the raw (UDP/TCP) transport.
            if (bytes.length >= 12 && bytes[0] === 0x11 && bytes[1] === SIGNAL_MAGIC) {
                this._handleSignal(bytes.subarray(1));
                return;
            }
            if (bytes.length >= SIGNAL_HEADER_LEN && bytes[0] === SIGNAL_MAGIC) {
                this._handleSignal(bytes);
                return;
            }

            // Voice packet from server: opcode 0x11 followed by VOICE_MAGIC 0x56, or direct 0x56
            if (bytes.length >= 16 && bytes[0] === 0x11 && bytes[1] === VOICE_MAGIC) {
                this.emit('voice', bytes.subarray(1));
                return;
            }
            if (bytes.length >= 15 && bytes[0] === VOICE_MAGIC) {
                this.emit('voice', bytes);
                return;
            }

            // QUIC-Style Ordered Event from Server SFU: opcode 0x11 + 0x03 or direct 0x03
            let evBytes = null;
            if (bytes.length === 15 && bytes[0] === 0x11 && bytes[1] === MeshOp.EVENT) {
                evBytes = bytes.subarray(1);
            } else if (bytes.length === 14 && bytes[0] === MeshOp.EVENT) {
                evBytes = bytes;
            }
            if (evBytes) {
                const ev = decodeMeshFrame(evBytes);
                if (ev) {
                    this._ackEvent(ev.seq);
                    this.emit('event', ev);
                    return;
                }
            }

            // CYG typed app message: opcode 0x11 + 0x05, or bare 0x05
            // on transports that do not prepend the wire opcode.
            if (bytes.length >= 3 && bytes[0] === 0x11 && bytes[1] === AppOp.MESSAGE) {
                this._handleAppMessage(bytes.subarray(1));
                return;
            }
            if (bytes.length >= 3 && bytes[0] === AppOp.MESSAGE) {
                this._handleAppMessage(bytes);
                return;
            }

            // App-level ping echo from server: [18][seq u32 LE]
            if (bytes.length === 5 && bytes[0] === 18) {
                const seq = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(1, true);
                const sentAt = this._pendingPings.get(seq);
                if (sentAt !== undefined) {
                    this._pendingPings.delete(seq);
                    this._onPingEcho(seq, sentAt, performance.now());
                    // Advance the ledger when the oldest sample is gone or has
                    // overstayed — not only when this echo WAS the oldest — or
                    // the loop's in-flight guard stalls on a dead seq.
                    if (this._oldestPendingSeq != null) {
                        const oldestAt = this._pendingPings.get(this._oldestPendingSeq);
                        if (oldestAt === undefined || performance.now() - oldestAt > 2000) {
                            this._advanceOldestPending();
                        }
                    }
                }
                return;
            }

            let offset = 0;
            if (bytes.length >= 15 && bytes[0] === 0x11 && bytes[1] === 0xA5) {
                offset = 7; // [opcode][0xA5 "delta"]
            } else if (bytes.length >= 14 && bytes[0] === 0xA5 && bytes[1] === 0x64) {
                offset = 6; // [0xA5 "delta"]
            }

            if (offset > 0) {
                this._processDeltaPayload(bytes.subarray(offset));
            }
        }

        /** Decode one CYG typed app message and dispatch its listeners. */
        _handleAppMessage(frame) {
            if (!frame || frame.length < 3) return;
            const typeLen = frame[1];
            if (typeLen === 0 || 2 + typeLen > frame.length) return;
            const type = _decoder
                ? _decoder.decode(frame.subarray(2, 2 + typeLen))
                : '';
            const payload = frame.subarray(2 + typeLen);

            // request/response replies pehle intercept: '__res' payload =
            // [reqId u16][data] — pending request resolve hota hai.
            if (type === '__res' && payload.length >= 2) {
                this._resolveRequest(type, payload);
                return;
            }

            // Generic listener first (useful for logging / routing), then the
            // type-specific handlers registered with onMessage(). The '*'
            // wildcard sees every typed message as (type, payload), Colyseus-style.
            this.emit('message', type, payload);
            const wildcard = this._messageHandlers.get('*');
            if (wildcard) {
                for (let i = 0; i < wildcard.length; i++) {
                    wildcard[i](type, payload, this);
                }
            }
            const handlers = this._messageHandlers.get(type);
            if (handlers) {
                for (let i = 0; i < handlers.length; i++) {
                    handlers[i](payload, this);
                }
            }
        }

        /**
         * One stamped `game-unreliable` delta frame from the server star.
         * Newest-wins supersession + fragment reassembly over the same wire
         * format as engine_webrtc_protocol::channel::FragmentAssembler, with one
         * deliberate difference: several deltas may reassemble concurrently, so
         * interleaved chunks of the next tick are not paid for by the last.
         * Returns true if this frame belonged to the delta channel.
         */
        _handleDeltaFrame(bytes) {
            const frame = parseDeltaFrame(bytes);
            if (!frame) return true; // malformed header: ours, but unusable
            if (!this._fragmentTimer) {
                // A delta that stopped making progress must not pin its chunks.
                this._fragmentTimer = setInterval(() => this.deltas.cleanup(), FRAGMENT_TTL_MS);
                // Never keep a test or headless process alive for a janitor.
                if (this._fragmentTimer.unref) this._fragmentTimer.unref();
            }
            const delta = this.deltas.push(frame);
            this.staleDeltas = this.deltas.staleCount;
            this.expiredDeltas = this.deltas.expiredCount;
            if (!delta) return true;
            this.reassembledDeltas = this.deltas.assembledCount;
            this.deltaSeq = delta.seq;
            this.deltaTick = delta.tick;
            // Delivery record: exactly one OP_ACK per reassembled delta — the
            // fragments coalesce into it, so loss of a chunk is never acked and
            // a delta is never acked twice. Reliable lane, so the server sees it.
            this._ackEvent(delta.seq);
            this._processDeltaPayload(delta.body);
            return true;
        }

        /** The sequencer's ledger for the HUD: what we applied, ignored and expired. */
        deltaStats() {
            return {
                reassembled: this.deltas.assembledCount,
                stale: this.deltas.staleCount,
                expired: this.deltas.expiredCount,
                inFlight: this.deltas.partials.size,
            };
        }

        /**
         * Body of one server delta: `[0xA5 'delta'][your_id i64][...game delta...]
         * [acked_seq u16][server_tick u32]`. The tail is Phase D reconciliation
         * data; handshake confirms and old servers carry none.
         *
         * The `game-unreliable` channel carries the whole body including the
         * `[0xA5 'delta']` magic, while the WebSocket path eats that magic with the
         * msgpack header. Both have to reach the game as the same bytes, so the magic
         * is dropped here and `your_id` is the first field on every path.
         */
        /**
         * Parse the one and only reconciliation tail:
         * `[acked u16][tick u32][stamp u16]` (8 bytes).
         *
         * There is no 6-byte fallback. A pre-Phase-D server is not a supported
         * peer; guessing at its tail silently misread `acked`/`tick` and made
         * prediction reconcile against garbage. Short frames now fail loudly.
         */
        _parseDeltaTail(payload) {
            if (payload.byteLength < DELTA_TAIL_LEN) {
                throw new Error(
                    `[CYG SDK] delta tail too short: ${payload.byteLength} bytes, `
                    + `need ${DELTA_TAIL_LEN}. Rebuild client and server together.`
                );
            }
            const pv = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
            const base = payload.byteLength - DELTA_TAIL_LEN;
            const stamp = pv.getUint16(base + 6, true);
            // Stamp 0 means "this client has not sent a stamped input yet"
            // (spectators, first frames). A non-zero stamp we do not remember
            // is still a valid tail; it just cannot produce an RTT sample.
            if (stamp !== 0 && this._stampTimes.has(stamp)) this._noteEcho(stamp);
            return {
                len: DELTA_TAIL_LEN,
                acked: pv.getUint16(base, true),
                tick: pv.getUint32(base + 2, true),
            };
        }

        /**
         * One RTT echo pair: the delta tail echoed a stamp we still hold a
         * send time for. RTT = now - send(stamp); `echoRtt` is the RFC 793
         * EMA (alpha 0.125), `echoJitter` the RFC 3550-style mean deviation
         * (|D| of consecutive sample deltas, same alpha).
         */
        _noteEcho(stamp) {
            const sentAt = this._stampTimes.get(stamp);
            if (sentAt === undefined) return;
            this._stampTimes.delete(stamp);
            const rtt = Math.max(0, performance.now() - sentAt);
            if (this._echoRtt < 0) {
                this._echoRtt = rtt;
            } else {
                // RFC 3550 jitter: |D| of the delta between consecutive
                // RTT samples, smoothed with the same alpha as the mean.
                const delta = Math.abs(rtt - this._lastEchoRtt);
                this._echoJitter += JITTER_DEV_ALPHA * (delta - this._echoJitter);
                this._echoRtt += RTT_EMA_ALPHA * (rtt - this._echoRtt);
            }
            this._lastEchoRtt = rtt;
            this.echoSamples++;
        }

        _setSessionId(value) {
            if (this.sessionId === value) return;
            this.sessionId = value;
            this.playerId = value == null ? null : Number(value);
            this.emit('sessionid', value);
        }

        _processDeltaPayload(raw, volatile = false) {
            let payload = raw;
            // A fragmented UDP volatile frame is reassembled by
            // DeltaSequencer with the 0xA6 header still inside the body.
            if (payload.length >= 5 && payload[0] === 0xA6) {
                const tick = new DataView(
                    payload.buffer,
                    payload.byteOffset + 1,
                    4
                ).getUint32(0, true);
                if (tick > this.lastVolatileTick) {
                    this.lastVolatileTick = tick;
                    this._processDeltaPayload(payload.subarray(5), true);
                }
                return;
            }
            if (payload.length >= 14 && payload[0] === 0xA5 && payload[1] === 0x64 && payload[2] === 0x65) {
                payload = payload.subarray(6); // 0xA5 'd' 'e' 'l' 't' 'a'
            }
            // Schema frame: [your_id i64]["OXS2"][kind...][acked u16][tick u32][stamp u16]
            if (payload.length >= 8 + 9 + 6
                && payload[8] === SCHEMA_MAGIC[0] && payload[9] === SCHEMA_MAGIC[1]
                && payload[10] === SCHEMA_MAGIC[2] && payload[11] === SCHEMA_MAGIC[3]) {
                const t = this._parseDeltaTail(payload);
                const acked = t.acked, tick = t.tick;
                if (volatile) {
                    // Volatile patches are absolute for their fields; the
                    // reliable stream owns the ack/tick ledger.
                    if (tick > this.lastVolatileTick) this.lastVolatileTick = tick;
                } else {
                    this.ackedSeq = acked;
                    this._trimInputHistory(acked);
                    this.serverTick = tick;
                    if (tick <= this.lastAppliedTick) return;
                    this.lastAppliedTick = tick;
                    // A full snapshot already contains the volatile values;
                    // ignore any older volatile frame that arrives after it.
                    const kind = payload[12];
                    if (kind === 5 || kind === 7) {
                        this.lastVolatileTick = Math.max(this.lastVolatileTick, tick);
                    }
                }
                const pv = new DataView(
                    payload.buffer,
                    payload.byteOffset,
                    payload.byteLength
                );
                this._setSessionId(pv.getBigInt64(0, true));
                const arrivalMs = performance.now();
                this.lastDeltaArrivalMs = arrivalMs;
                this.timeSync.setTickRate(this.tickRate);
                this.timeSync.addSample(arrivalMs, tick);
                this.tickOriginMs = this.timeSync.tickOriginMs;
                this.clockOffsetMs = this.timeSync.clockOffsetMs(arrivalMs);
                // Client prediction reconcile: the authoritative body (tail
                // stripped) replays the unacked pending inputs on the guest.
                if (!volatile && this._predict && this._predict.available) {
                    this._predict.onAck(payload.subarray(0, payload.byteLength - t.len));
                }
                // Schema decode happens in cyg-core-rs (wasm): it unwraps
                // the envelope, decodes the OXS2 section and stores the
                // state; cyg.js hydrates its event mirror afterwards.
                this._applySchemaViaCore(raw);
                return;
            }

            let ackedSeq = -1;
            let serverTick = -1;
            if (payload.length >= 23 && (payload[0] === 0xA5 || payload[8] === 1)) {
                // Game-delta body (version byte at [8]); `0xA5` covers a body
                // that still carries the msgpack type header. Voice ('V'/'W')
                // and other frames never satisfy both conditions.
                const t = this._parseDeltaTail(payload);
                ackedSeq = t.acked;
                serverTick = t.tick;
                this.ackedSeq = ackedSeq;
                this._trimInputHistory(ackedSeq);
                this.serverTick = serverTick;
                payload = payload.subarray(0, payload.byteLength - t.len);
            }

            // SeqFilter: a game delta whose tick does not advance the last applied
            // tick is superseded (out-of-order/late) and must never reach the game.
            // Volatile frames have their own newest-tick gate above.
            if (!volatile && serverTick >= 0 && serverTick <= this.lastAppliedTick) return;

            if (payload.length >= 8) {
                const v = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
                this._setSessionId(v.getBigInt64(0, true));
            }

            if (payload.length >= 15 && payload[0] === VOICE_MAGIC) {
                this.emit('voice', payload);
                return;
            }

            // Capture payload if currently recording
            if (this.recording) {
                this.recordedFrames.push({
                    t: Math.round(performance.now() - this.recordStartTime),
                    d: Array.from(payload)
                });
            }

            const arrivalMs = performance.now();
            this.lastDeltaArrivalMs = arrivalMs;
            if (serverTick >= 0 && !volatile) {
                this.lastAppliedTick = serverTick;
                this.timeSync.setTickRate(this.tickRate);
                this.timeSync.addSample(arrivalMs, serverTick);
                this.tickOriginMs = this.timeSync.tickOriginMs;
                this.clockOffsetMs = this.timeSync.clockOffsetMs(arrivalMs);
            }
            if (this.timeline && this._deltaDecoder && serverTick >= 0) {
                try {
                    this.timeline.pushSnapshot(serverTick, this._deltaDecoder(payload), arrivalMs);
                } catch (e) { /* decoder threw: the 'delta' event still fires */ }
            }
            if (this.jitterBufferEnabled) {
                this._jitterQueue.push({ payload, ackedSeq, serverTick, arrivalMs, deliverAt: arrivalMs + this.jitterDelayMs });
            } else {
                this.emit('delta', payload, ackedSeq, serverTick);
            }
        }

        /**
         * Stream a 15-byte header + Opus voice packet to the server
         */
        sendVoice(channelMode, seq, timestamp, flags, opusPayload) {
            if (this.isSpectator) return;
            const totalLen = 15 + opusPayload.length;
            const buf = new Uint8Array(totalLen);
            const view = new DataView(buf.buffer);
            buf[0] = VOICE_MAGIC; // 'V'
            view.setUint32(1, 0, false);
            view.setUint16(5, seq, false);
            view.setUint32(7, timestamp, false);
            buf[11] = channelMode; // 0=Proximity, 1=Global, 2=TeamOnly, 3=Whisper
            buf[12] = flags;
            view.setUint16(13, opusPayload.length, false);
            buf.set(opusPayload, 15);
            // RTC active: voice is loss-tolerant, so prefer the unreliable lane
            // and fall back to the reliable one before the WS.
            if (this.serverRtcActive) {
                const chUnreliable = this.serverChUnreliable;
                if (chUnreliable && chUnreliable.readyState === 'open') {
                    try { chUnreliable.send(buf); return; } catch (e) { /* closing */ }
                }
                const chReliable = this.serverChReliable;
                if (chReliable && chReliable.readyState === 'open') {
                    try { chReliable.send(buf); return; } catch (e) { /* closing */ }
                }
            }
            if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
            this.ws.send(buf);
        }

        /**
         * Set the current squad/team on the server voice router.
         *
         * `teamId` may be a number or a string (`"red"`, `"squad-a"`). The
         * label is hashed with the same FNV-1a used by the schema, so the wire
         * stays a fixed-width u32 and routing is one integer compare per
         * packet. Pass `null`/`""` to leave the team.
         */
        setVoiceTeam(teamId) {
            if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
            const label = (teamId == null) ? "" : String(teamId).trim();
            const hash = label.length === 0 ? 0 : schemaFnv1a(2166136261, label);
            const buf = new Uint8Array(6);
            buf[0] = VOICE_MAGIC;
            buf[1] = 0x54; // 'T'
            buf[2] = (hash >>> 24) & 0xFF;
            buf[3] = (hash >>> 16) & 0xFF;
            buf[4] = (hash >>> 8) & 0xFF;
            buf[5] = hash & 0xFF;
            this.ws.send(buf);
        }

        /**
         * Notify server of microphone mute status
         */
        setVoiceMuted(isMuted) {
            if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
            this.ws.send(new Uint8Array([VOICE_MAGIC, 0x4D, isMuted ? 1 : 0])); // 'V', 'M', 1/0
        }

        /**
         * Notify server of sound deafen status (cannot hear anyone)
         */
        setVoiceDeafened(isDeafened) {
            if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
            this.ws.send(new Uint8Array([VOICE_MAGIC, 0x44, isDeafened ? 1 : 0])); // 'V', 'D', 1/0
        }

        /**
         * Send a WebRTC signaling frame. The server stamps the sender from its own
         * session map and only forwards the body, so `to` is the sole routing input
         * a client controls.
         */
        sendSignal(kind, to, body) {
            if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return false;
            this.ws.send(encodeSignalFrame(kind, to, body));
            return true;
        }

        /** Decode an inbound signal and hand it to the attached voice mesh or server WebRTC session. */
        _handleSignal(bytes) {
            const frame = decodeSignalFrame(bytes);
            if (!frame) return;

            // Direct server WebRTC signaling (frame.from === 0 / SERVER_SENDER)
            if (frame.from === 0) {
                if (frame.kind === SignalKind.ANSWER && this.serverPc) {
                    const answer = frame.body;
                    if (answer && answer.sdp) {
                        this.serverPc.setRemoteDescription(new RTCSessionDescription({
                            type: 'answer',
                            sdp: answer.sdp
                        })).then(() => {
                            console.log('[CYG SDK] ⚡ WebRTC Server SDP Answer accepted!');
                        }).catch(e => {
                            console.warn('[CYG SDK] setRemoteDescription failed:', e);
                        });
                    }
                    return;
                } else if (frame.kind === SignalKind.ICE && this.serverPc) {
                    if (frame.body && frame.body.candidate) {
                        this.serverPc.addIceCandidate(new RTCIceCandidate({
                            candidate: frame.body.candidate,
                            sdpMid: frame.body.sdpMid,
                            sdpMLineIndex: frame.body.sdpMLineIndex
                        })).catch(e => console.warn('[CYG SDK] addIceCandidate failed:', e));
                    }
                    return;
                }
            }

            if (this.voice) this.voice._onSignal(frame);
            this.emit('signal', frame);
        }

        /**
         * Attach a WebRTC voice mesh to this room. Returns the mesh, which then
         * receives every signaling frame the server relays.
         */
        attachVoice(options = {}) {
            this.voice = new CYGVoiceMesh(this, options);
            return this.voice;
        }

        /**
         * Colyseus parity: Room.input() handle.
         * Creates or retrieves the input handle for this room.
         */
        input(options = {}) {
            if (this._inputHandle) return this._inputHandle;
            const self = this;
            const data = {
                seq: 0,
                flags: 0,
                aimYaw: 0,
                aimPitch: 0,
                moveX: 0,
                moveZ: 0,
                jump: false,
                up: false,
                down: false,
                left: false,
                right: false
            };
            const tickRate = options.tickRate > 0 ? options.tickRate : 0;
            const handle = {
                data,
                mode: options.mode || 'unreliable',
                tickRate,
                _accMs: 0,
                _lastTickMs: -1,
                /** Build the schema-declared input object from handle state. */
                _build() {
                    if (!self._inputSchema) {
                        if (!self._inputLaneWarned) {
                            self._inputLaneWarned = true;
                            console.warn('[CYG SDK] room.input() needs a schema-declared '
                                + '`Input` def — add `export const Input = schema({…}, "Input")` '
                                + 'to src/schema/ and sendInput({ moveX, moveZ, … }) starts working');
                        }
                        return null;
                    }
                    let flags = data.flags || 0;
                    if (data.up) flags |= 1;
                    if (data.down) flags |= 2;
                    if (data.left) flags |= 4;
                    if (data.right) flags |= 8;
                    if (data.jump) flags |= 16;
                    let mx = data.moveX || 0;
                    let mz = data.moveZ || 0;
                    if (mx === 0 && mz === 0) {
                        if (data.left) mx -= 1;
                        if (data.right) mx += 1;
                        if (data.up) mz -= 1;
                        if (data.down) mz += 1;
                    }
                    const willJump = !!(data.jump || data._jumpLatched);
                    data._jumpLatched = false;
                    const obj = {};
                    for (const f of self._inputSchema.fields) {
                        if (f.name === 'moveX') obj.moveX = mx;
                        else if (f.name === 'moveZ') obj.moveZ = mz;
                        else if (f.name === 'jump') obj.jump = willJump;
                        else if (f.name === 'flags') obj.flags = flags;
                        else if (f.name === 'aimYaw') obj.aimYaw = data.aimYaw || 0;
                        else if (f.name === 'aimPitch') obj.aimPitch = data.aimPitch || 0;
                        else obj[f.name] = data[f.name] || 0;
                    }
                    return obj;
                },
                /** Send one input frame now. When a prediction guest is
                 *  attached the same frame advances `cyg_predict_tick`, so
                 *  network and prediction never diverge. */
                send(dtMs = 1000 / Math.max(1, (self.timeSync && self.timeSync.tickRate) || 30)) {
                    const obj = handle._build();
                    if (!obj) return false;
                    return self._sendInputObject(obj, dtMs);
                },
                /** Fixed-step input pump: send/predict on a fixed timeline
                 *  regardless of render frame rate. Returns steps consumed. */
                tick(nowMs = performance.now()) {
                    if (!self._inputSchema) { handle._build(); return 0; }
                    const rate = tickRate > 0 ? tickRate
                        : (self.timeSync && self.timeSync.tickRate ? self.timeSync.tickRate : 30);
                    const stepMs = 1000 / Math.max(1, rate || 30);
                    const elapsed = handle._lastTickMs < 0 ? 0 : nowMs - handle._lastTickMs;
                    handle._lastTickMs = nowMs;
                    handle._accMs += Math.min(250, Math.max(0, elapsed));
                    let steps = 0;
                    while (handle._accMs >= stepMs) {
                        handle._accMs -= stepMs;
                        const obj = handle._build();
                        if (!obj) break;
                        if (self._sendInputObject(obj, stepMs)) steps++;
                    }
                    return steps;
                },
                reset() {
                    handle._accMs = 0;
                    handle._lastTickMs = -1;
                },
                bindKeyboard(target = (typeof window !== 'undefined' ? window : null)) {
                    if (!target) return () => {};
                    const onKey = (e, down) => {
                        let consumed = false;
                        if (e.code === 'KeyW' || e.code === 'ArrowUp') { data.up = down; consumed = true; }
                        if (e.code === 'KeyS' || e.code === 'ArrowDown') { data.down = down; consumed = true; }
                        if (e.code === 'KeyA' || e.code === 'ArrowLeft') { data.left = down; consumed = true; }
                        if (e.code === 'KeyD' || e.code === 'ArrowRight') { data.right = down; consumed = true; }
                        if (e.code === 'Space') {
                            data.jump = down;
                            if (down) data._jumpLatched = true;
                            consumed = true;
                            if (down && typeof e.preventDefault === 'function') e.preventDefault();
                        }
                        if (consumed) {
                            data.moveX = (data.right ? 1 : 0) - (data.left ? 1 : 0);
                            data.moveZ = (data.down ? 1 : 0) - (data.up ? 1 : 0);
                        }
                    };
                    const downHandler = (e) => onKey(e, true);
                    const upHandler = (e) => onKey(e, false);
                    target.addEventListener('keydown', downHandler);
                    target.addEventListener('keyup', upHandler);
                    return () => {
                        target.removeEventListener('keydown', downHandler);
                        target.removeEventListener('keyup', upHandler);
                    };
                }
            };
            this._inputHandle = handle;
            return handle;
        }

        /** Internal: one already-built input object. Routes through the
         *  predict driver when one is attached so the sent frame and the
         *  predicted frame carry the same sequence number and bytes. */
        _sendInputObject(obj, dtMs) {
            if (this._predict && this._predict.available
                && typeof this._predict.sendSchema === 'function') {
                return this._predict.sendSchema(obj, dtMs);
            }
            return this._sendSchemaInput(obj);
        }

        /**
         * Schema input lane — the project's `Input` def from the descriptor
         * drives the packing, so the game only ever passes a plain object:
         *   room.sendInput({ moveX, moveZ, jump, skill })
         * Wire: one typed app message ("__input", reserved) carrying
         * [seq u16 LE][field bytes in schema order]; the guest decodes it back
         * into the same generated class and drops stale frames by seq.
         */
        _packSchemaInput(obj) {
            const def = this._inputSchema;
            if (!def || !def.fields || def.fields.length === 0) return null;
            if (!this._core) return null; // wasm core not loaded yet
            this.inputSeq = (this.inputSeq + 1) & 0xFFFF;
            // The Input schema's field order drives the packing — the values
            // are handed to cyg-core-rs as raw i64 bit patterns and the core
            // reinterprets them per the descriptor's field kinds.
            const values = [];
            for (const f of def.fields) {
                const v = obj ? obj[f.name] : undefined;
                const num = Number(v) || 0;
                switch (f.type) {
                    case 'bool': values.push(BigInt(v ? 1 : 0)); break;
                    case 'f32': {
                        const f32 = new Float32Array(1); f32[0] = num;
                        values.push(BigInt(new Int32Array(f32.buffer)[0]));
                        break;
                    }
                    case 'f64': {
                        const f64 = new Float64Array(1); f64[0] = num;
                        values.push(new BigInt64Array(f64.buffer)[0]);
                        break;
                    }
                    case 'i64': values.push(BigInt(Math.trunc(num))); break;
                    default: values.push(BigInt(Math.trunc(num))); break;
                }
            }
            return this._core.encodeSchemaInput(this._coreHandle, this.inputSeq, values);
        }

        _sendSchemaInput(obj) {
            if (this.isSpectator) return false;
            const body = this._packSchemaInput(obj);
            if (!body) return false;
            return this._sendPackedInput(body);
        }

        /** Send one already-packed `[seq u16][fields…]` input body. */
        _sendPackedInput(packed) {
            if (this.isSpectator) return false;
            if (!packed || packed.byteLength < 2) return false;
            this._rememberInput(packed);
            let body = packed;
            let type = '__input';
            const batch = this._buildInputBatch();
            if (batch) {
                body = batch;
                type = '__input_batch';
            }
            body = this._appendInputStamp(body);
            const frame = this._encodeAppMessage(type, body);
            if (this.serverChUnreliable && this.serverChUnreliable.readyState === 'open') {
                this.serverRtcActive = true;
                const copies = Math.max(1, Math.min(4, this.inputRedundancy || 1));
                let sent = 0;
                for (let i = 0; i < copies; i++) {
                    try { this.serverChUnreliable.send(frame); sent++; }
                    catch (e) { break; /* closing */ }
                }
                if (sent > 0) {
                    this.bytesOut += frame.byteLength * sent;
                    return true;
                }
            }
            if (this.serverChReliable && this.serverChReliable.readyState === 'open') {
                try {
                    this.serverChReliable.send(frame);
                    this.bytesOut += frame.byteLength;
                    return true;
                } catch (e) { /* closing */ }
            }
            // WebRTC not negotiated yet — the WS lane reaches the same guest dispatch.
            return this.send(type, body);
        }

        /** Add one sent body to the short replay ring. */
        _rememberInput(packed) {
            if (!this._inputHistory) this._inputHistory = [];
            const seq = packed[0] | (packed[1] << 8);
            this._inputHistory.push({
                seq,
                frame: packed.subarray(2).slice(),
                atMs: performance.now(),
            });
            const keep = Math.max(1, (this.inputHistory | 0) + 1);
            while (this._inputHistory.length > keep) this._inputHistory.shift();
            this._trimInputHistory(this.ackedSeq);
        }

        /** Drop frames the server has already acknowledged (wrap-safe). */
        _trimInputHistory(acked) {
            if (!this._inputHistory || !(acked >= 0)) return;
            while (this._inputHistory.length > 0) {
                const seq = this._inputHistory[0].seq;
                const age = (acked - seq) & 0xffff;
                if (age >= 32768) break; // this frame is newer than the ack
                this._inputHistory.shift();
            }
        }

        /**
         * Build `[count][seq u16][len u16][frame]…` from the replay ring.
         * Returns null when there is nothing to replay or the batch would
         * exceed the host's 512-byte cap; the caller then sends one frame.
         */
        _buildInputBatch() {
            if (!this._inputHistory || (this.inputHistory | 0) <= 0) return null;
            if (this._inputHistory.length < 2) return null;
            let frames = this._inputHistory;
            let total = 1;
            for (const f of frames) total += 4 + f.frame.byteLength;
            while (frames.length > 1 && total > 512) {
                total -= 4 + frames[0].frame.byteLength;
                frames = frames.slice(1);
            }
            if (frames.length < 2 || total > 512) return null;
            const body = new Uint8Array(total);
            body[0] = frames.length & 0xff;
            let off = 1;
            for (const f of frames) {
                body[off] = f.seq & 0xff;
                body[off + 1] = (f.seq >>> 8) & 0xff;
                body[off + 2] = f.frame.byteLength & 0xff;
                body[off + 3] = (f.frame.byteLength >>> 8) & 0xff;
                body.set(f.frame, off + 4);
                off += 4 + f.frame.byteLength;
            }
            return body;
        }

        /** Append the rotating RTT stamp trailer and remember its send time. */
        _appendInputStamp(body) {
            const stamp = (this._stampSeq = (this._stampSeq + 1) & 0xFFFF) >>> 0;
            const stamped = new Uint8Array(body.byteLength + INPUT_STAMP_LEN);
            stamped.set(body, 0);
            stamped[body.byteLength] = INPUT_STAMP_MAGIC[0];
            stamped[body.byteLength + 1] = INPUT_STAMP_MAGIC[1];
            stamped[body.byteLength + 2] = stamp & 0xFF;
            stamped[body.byteLength + 3] = (stamp >>> 8) & 0xFF;
            // Server-time estimate from the client's TimeSync. 0 means "not
            // synchronized yet"; the guest falls back to its current time.
            const synced = this.timeSync && this.timeSync.synced;
            const clientMs = synced
                ? Math.min(0xFFFFFFFF, Math.max(0, Math.round(this.serverNow())))
                : 0;
            stamped[body.byteLength + 4] = clientMs & 0xFF;
            stamped[body.byteLength + 5] = (clientMs >>> 8) & 0xFF;
            stamped[body.byteLength + 6] = (clientMs >>> 16) & 0xFF;
            stamped[body.byteLength + 7] = (clientMs >>> 24) & 0xFF;
            if (!this._stampTimes) this._stampTimes = new Map();
            if (!this._stampOrder) this._stampOrder = [];
            this._stampTimes.set(stamp, performance.now());
            this._stampOrder.push(stamp);
            while (this._stampOrder.length > 256) {
                const evicted = this._stampOrder.shift();
                this._stampTimes.delete(evicted);
            }
            return stamped;
        }

        /**
         * Typed input lane — pass the object matching your schema's `Input`
         * def; the SDK packs it (field order = wire order) and sends it on
         * the newest-wins unreliable lane:
         *   room.sendInput({ moveX, moveZ, jump, skill })
         * Declaring the def is required — `export const Input = schema({...}, "Input")`
         * in src/schema/, then `room.input()` + `bindKeyboard()` fill it for you.
         */
        sendInput(input) {
            if (input === null || typeof input !== 'object') {
                throw new TypeError(
                    '[CYG SDK] sendInput(input) takes the schema-declared input object — '
                    + 'declare `export const Input = schema({…}, "Input")` and pass e.g. '
                    + 'room.sendInput({ moveX, moveZ, jump })'
                );
            }
            return this._sendSchemaInput(input);
        }

        /**
         * Send a 14-byte QUIC-style reliable ordered event frame to the server SFU
         */
        sendEvent(kind, target = 0xFFFF, tick = 0, value = 0.0) {
            if (this.isSpectator) {
                return;
            }

            this.eventSeq = (this.eventSeq + 1) & 0xFFFF;
            const buf = encodeEventFrame({
                seq: this.eventSeq,
                kind,
                from: (this.sessionId != null) ? Number(this.sessionId) : 0,
                target,
                tick,
                value
            });

            // Pure WebRTC DataChannel (SCTP guaranteed ordered delivery):
            // When WebRTC is active, NO event message is sent on WebSocket!
            if (this.serverChReliable && this.serverChReliable.readyState === 'open') {
                try {
                    this.serverChReliable.send(buf);
                    return;
                } catch (e) {
                    console.warn('[CYG SDK] serverChReliable.send error:', e);
                }
            }

            // Fallback to WebSocket only while WebRTC connection is negotiating
            if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                this.ws.send(buf);
            }
        }

        leave(consented = true) {
            this._leaveCode = consented ? 1000 : 1001;
            this._intentionalClose = true;
            // Nobody is coming to answer these: reject every pending RPC so
            // callers don't wait out the full timeout.
            for (const pending of this._pendingRequests.values()) {
                clearTimeout(pending.timer);
                pending.reject(new Error('[CYG SDK] room closed before the request resolved'));
            }
            this._pendingRequests.clear();
            if (this._signalJoinTimer) {
                clearTimeout(this._signalJoinTimer);
                this._signalJoinTimer = null;
            }
            // Release the wasm core slot for this room.
            if (this._core) {
                this._core.free(this._coreHandle);
                this._coreHandle = 0;
                this._core = null;
            }
            // Colyseus parity: resolves once the close handshake completes
            // (or immediately when there is nothing left to close).
            const closed = new Promise((resolve) => {
                if (!this.isOpen && !this.ws) { resolve(this._leaveCode); return; }
                const onClosed = () => resolve(this._leaveCode);
                this.on('leave', onClosed);
                setTimeout(() => resolve(this._leaveCode), 3000); // safety net
            });
            if (this._reconnectTimer) {
                clearTimeout(this._reconnectTimer);
                this._reconnectTimer = null;
            }
            this.serverRtcActive = false;
            if (this._fragmentTimer) {
                clearInterval(this._fragmentTimer);
                this._fragmentTimer = null;
            }
            if (this.serverChReliable) {
                try {
                    if (this.serverChReliable.readyState === 'open' && consented) {
                        this.serverChReliable.send(new Uint8Array([12]));
                    }
                    this.serverChReliable.close();
                } catch (_) {}
                this.serverChReliable = null;
            }
            if (this.serverChUnreliable) {
                try { this.serverChUnreliable.close(); } catch (_) {}
                this.serverChUnreliable = null;
            }
            if (this.signalWs) {
                try { this.signalWs.close(1000, "leave"); } catch (_) {}
                this.signalWs = null;
            }
            if (this.serverPc) {
                try { this.serverPc.close(); } catch (_) {}
                this.serverPc = null;
            }
            if (this.ws) {
                try {
                    if (this.ws.readyState === WebSocket.OPEN && consented) {
                        this.ws.send(new Uint8Array([12]));
                    }
                    this.ws.close(consented ? 4000 : 1000, "leave");
                } catch (_) {}
                this.ws = null;
            }
            if (this.voice) {
                this.voice.dispose();
            }
            this.emit('leave', this._leaveCode);
            return closed;
        }

        // --- SCHEMA STATE SYNC ---

        /**
         * Attach a schema definition. After this call the room owns
         * `room.state` (a Map of id -> entity) and decodes full/patch frames
         * automatically. The definition must match the server's `SchemaDef`
         * field-for-field; a hash mismatch is reported through `schemaerror`.
         *
         * Not required any more: when the server ships an `cyg.schema.bin`
         * descriptor, the client attaches it on join automatically.
         */
        useSchema(schema) {
            this.schema = schema;
            this.schemas = [schema];
            this._rootScalars = null;
            this._rootValues = {};
            const name = this._rootCollection || 'players';
            this.state = new SchemaMap(this, name, schema);
            this._collectionMaps = new Map([[name, this.state]]);
            this._collections = [{ id: 0, name, map: this.state, schema }];
            this._entityCallbacks = new WeakMap();
            this._schemaReady = false;
            this._schemaAcked = false;
            this._schemaError = null;
            this._defineStateProperties();
            return this;
        }

        /**
         * Attach an `cyg.schema.bin` descriptor locally (benchmarks and
         * offline clients). Live rooms receive it from the host automatically.
         */
        useDescriptor(bytes) {
            const descriptor = bytes instanceof ArrayBuffer ? new Uint8Array(bytes) : bytes;
            if (!isSchemaDescriptor(descriptor)) {
                throw new TypeError('[CYG SDK] useDescriptor() expects an OXSD buffer');
            }
            if (descriptor[4] !== DESCRIPTOR_VERSION) {
                throw new TypeError(
                    `[CYG SDK] unsupported schema descriptor v${descriptor[4]}; `
                    + `this client speaks v${DESCRIPTOR_VERSION}. Rebuild client and server together.`
                );
            }
            this._attachDescriptor(descriptor);
            return this;
        }

        /** Primary (first) collection name from the descriptor. */
        get rootCollection() {
            return this._rootCollection;
        }

        /** Expose collections and root scalars as properties of `room.state`. */
        _defineStateProperties() {
            const nestedNames = new Set((this._nested || []).map(n => n.collection));
            for (const [name, map] of this._collectionMaps) {
                if (nestedNames.has(name)) continue;
                Object.defineProperty(this.state, name, {
                    value: map,
                    enumerable: false,
                    configurable: true
                });
            }
            if (this._rootScalars) {
                for (const field of this._rootScalars.fields) {
                    Object.defineProperty(this.state, field.name, {
                        get: () => this._rootValues[field.name],
                        enumerable: true,
                        configurable: true
                    });
                }
            }
        }

        /** Build the client contract from the host's "OXSD" descriptor frame.
         *  The parse itself happens in cyg-core-rs (cyg_create); we read the
         *  parsed contract back through the wasm ABI and mirror it in JS. */
        _attachDescriptor(bytes) {
            if (this._descriptorAttached) return;
            this._descriptorAttached = true;
            this._descriptorBytes = bytes;
            if (bytes.length < 6 || bytes[4] !== DESCRIPTOR_VERSION) {
                const message = `[CYG SDK] unsupported schema descriptor v${bytes.length > 4 ? bytes[4] : '?'}; `
                    + `this client speaks v${DESCRIPTOR_VERSION}. Rebuild client and server together.`;
                this._schemaError = message;
                this.emit('schemaerror', message);
                return;
            }
            this._corePromise.then((core) => {
                if (!core) {
                    const message = '[CYG SDK] cyg_core_rs.wasm failed to load: '
                        + (this._coreError || 'unknown error');
                    this._schemaError = message;
                    this.emit('schemaerror', message);
                    return;
                }
                if (this._core) return; // already attached (reconnect)
                const handle = core.create(bytes);
                if (!handle) {
                    const message = '[CYG SDK] invalid schema descriptor from server';
                    this._schemaError = message;
                    this.emit('schemaerror', message);
                    return;
                }
                this._core = core;
                this._coreHandle = handle;
                this._buildSchemaMirror();
                const queued = this._pendingSchema;
                this._pendingSchema = [];
                for (const payload of queued) this._applySchemaViaCore(payload);
            }).catch((err) => {
                const message = '[CYG SDK] schema attach failed: ' + ((err && err.message) || err);
                this._schemaError = message;
                this.emit('schemaerror', message);
            });
        }

        /**
         * Mirror the core's parsed descriptor into JS contract objects
         * (SchemaDef instances with identical name/version/fields/hash) so
         * the public room.schema / room.state surface stays unchanged.
         */
        _buildSchemaMirror() {
            const core = this._core;
            const handle = this._coreHandle;
            const schemas = [];
            for (let i = 0; i < core.schemaCount(handle); i++) {
                const name = core.schemaName(handle, i);
                if (!name) continue;
                const def = new SchemaDef(name, core.schemaVersion(handle, i));
                const fieldCount = core.schemaFieldCount(handle, i);
                for (let f = 0; f < fieldCount; f++) {
                    const fieldName = core.schemaFieldName(handle, i, f);
                    const kind = core.schemaFieldKind(handle, i, f);
                    const flags = core.schemaFieldFlags(handle, i, f);
                    const elementKind = core.schemaFieldElementKind(handle, i, f);
                    const type = CORE_KIND_TO_TYPE[kind];
                    const element = elementKind ? CORE_KIND_TO_TYPE[elementKind] : null;
                    const nested = typeof core.schemaFieldNested === 'function'
                        ? core.schemaFieldNested(handle, i, f) : null;
                    if (fieldName && type) def._add(fieldName, type, flags, element, nested);
                }
                // The wire hash comes from the core; the JS re-hash must land
                // on the same value for section hash checks to stay visible.
                def.hash = core.schemaHash(handle, i);
                schemas.push(def);
            }

            const collectionCount = core.collectionCount(handle);
            if (!collectionCount || !schemas.length) {
                const message = '[CYG SDK] invalid schema descriptor from server';
                this._schemaError = message;
                this.emit('schemaerror', message);
                return;
            }
            const collections = [];
            for (let c = 0; c < collectionCount; c++) {
                const name = core.collectionName(handle, c);
                const schema = schemas.find(s => s.hash === this._collectionSchemaHash(c)) || null;
                if (!name || !schema) {
                    const message = `[CYG SDK] collection '${name || c}' has no matching schema`;
                    this._schemaError = message;
                    this.emit('schemaerror', message);
                    return;
                }
                collections.push({ name, schema });
            }

            const nested = [];
            const nestedCount = core.nestedCount(handle);
            for (let i = 0; i < nestedCount; i++) {
                const collection = core.nestedCollection(handle, i);
                const parent = core.nestedParent(handle, i);
                const field = core.nestedField(handle, i);
                if (collection && parent && field) nested.push({ collection, parent, field });
            }
            this._nested = nested;

            const first = collections[0];
            this.schemas = schemas;
            this._inputSchema = schemas.find(s => s.name === 'Input') || null;
            this._rootCollection = first.name;

            const rootFieldCount = core.rootFieldCount(handle);
            let rootScalars = null;
            if (rootFieldCount > 0) {
                rootScalars = new SchemaDef(
                    core.rootSchemaName(handle) || 'State', core.rootSchemaVersion(handle));
                for (let f = 0; f < rootFieldCount; f++) {
                    const fieldName = core.rootFieldName(handle, f);
                    const kind = core.rootFieldKind(handle, f);
                    if (fieldName && CORE_KIND_TO_TYPE[kind]) {
                        rootScalars._add(fieldName, CORE_KIND_TO_TYPE[kind]);
                    }
                }
                rootScalars.hash = 0; // root hash is checked inside the core
            }
            this._rootScalars = rootScalars;

            const oldState = this.state;
            const oldCallbacks = this._entityCallbacks.get(this.state);
            this.schema = first.schema;
            this.state = new SchemaMap(this, first.name, first.schema);
            if (oldState) {
                if (oldState._onAddHandlers) this.state._onAddHandlers.push(...oldState._onAddHandlers);
                if (oldState._onRemoveHandlers) this.state._onRemoveHandlers.push(...oldState._onRemoveHandlers);
                if (oldState._onChangeHandlers) this.state._onChangeHandlers.push(...oldState._onChangeHandlers);
            }
            this._entityCallbacks = new WeakMap();
            this._collectionMaps = new Map();
            this._collections = [];
            this._rootValues = {};
            if (rootScalars) {
                for (const field of rootScalars.fields) {
                    this._rootValues[field.name] = this._schemaDefaultValue(field);
                }
                this._trackEntity(this.state);
                if (oldCallbacks) {
                    const newMeta = this._entityCallbacks.get(this.state);
                    if (newMeta) {
                        Object.assign(newMeta.fields, oldCallbacks.fields);
                        newMeta.any.push(...oldCallbacks.any);
                    }
                }
            }
            for (let i = 0; i < collections.length; i++) {
                const entry = collections[i];
                const map = i === 0 ? this.state : new SchemaMap(this, entry.name, entry.schema);
                this._collectionMaps.set(entry.name, map);
                this._collections.push({ id: i, name: entry.name, map, schema: entry.schema });
            }
            this._schemaReady = false;
            this._schemaAcked = false;
            this._schemaError = null;
            this._defineStateProperties();
        }

        /** Wire hash of collection `id` as the core resolved it. */
        _collectionSchemaHash(id) {
            return this._core.collectionSchemaHash(this._coreHandle, id);
        }

        /** The shared callback facade (also `Callbacks.get(room)`). */
        _callbacks() {
            return (this._schemaCallbacks ||= new SchemaCallbacks(this));
        }

        _trackEntity(entity) {
            this._entityCallbacks.set(entity, {
                fields: new Map(),
                any: []
            });
            // Colyseus proxy parity: entities carry listen/onChange/bindTo
            // directly, so `(player) => player.listen("x", cb)` works inside
            // an onAdd handler without reaching for the callbacks object.
            // SchemaMap instances (collections) have their own onAdd/onChange
            // methods — the proxy form is for plain decoded entities only.
            if (entity !== null && typeof entity === 'object' &&
                !(entity instanceof Map) &&
                !Object.prototype.hasOwnProperty.call(entity, 'listen')) {
                const room = this;
                Object.defineProperties(entity, {
                    listen: {
                        value: (prop, handler, immediate) =>
                            room._callbacks().listen(entity, prop, handler, immediate),
                        enumerable: false, configurable: true, writable: true
                    },
                    onChange: {
                        value: (handler, immediate) =>
                            room._callbacks().onChange(entity, handler, immediate),
                        enumerable: false, configurable: true, writable: true
                    },
                    bindTo: {
                        value: (to, properties) =>
                            room._callbacks().bindTo(entity, to, properties),
                        enumerable: false, configurable: true, writable: true
                    }
                });
            }
        }

        _fireAdd(map, entity, id) {
            const previous = this._isTriggering;
            this._isTriggering = true;
            try {
                for (let i = 0; i < map._onAddHandlers.length; i++) {
                    try { map._onAddHandlers[i](entity, id); } catch (e) { this.emit('error', e); }
                }
                for (let i = 0; i < this._onAddHandlers.length; i++) {
                    try { this._onAddHandlers[i](entity, id); } catch (e) { this.emit('error', e); }
                }
            } finally {
                this._isTriggering = previous;
            }
        }

        _fireRemove(map, entity, id) {
            const previous = this._isTriggering;
            this._isTriggering = true;
            try {
                for (let i = 0; i < map._onRemoveHandlers.length; i++) {
                    try { map._onRemoveHandlers[i](entity, id); } catch (e) { this.emit('error', e); }
                }
                for (let i = 0; i < this._onRemoveHandlers.length; i++) {
                    try { this._onRemoveHandlers[i](entity, id); } catch (e) { this.emit('error', e); }
                }
            } finally {
                this._isTriggering = previous;
            }
        }

        _fireFieldChange(entity, field, value, previousValue) {
            const previous = this._isTriggering;
            this._isTriggering = true;
            try {
                const meta = this._entityCallbacks.get(entity);
                const listeners = meta && meta.fields.get(field);
                if (listeners) {
                    for (let i = 0; i < listeners.length; i++) {
                        try { listeners[i](value, previousValue); } catch (e) { this.emit('error', e); }
                    }
                }
                for (let i = 0; i < this._onChangeHandlers.length; i++) {
                    try { this._onChangeHandlers[i](entity, field, value); } catch (e) { this.emit('error', e); }
                }
            } finally {
                this._isTriggering = previous;
            }
        }

        /** Entity-level + collection-level change fan-out for one collection. */
        _fireUpdate(map, entity, id) {
            const previous = this._isTriggering;
            this._isTriggering = true;
            try {
                const meta = this._entityCallbacks.get(entity);
                if (meta) {
                    for (let i = 0; i < meta.any.length; i++) {
                        try { meta.any[i](); } catch (e) { this.emit('error', e); }
                    }
                }
                for (let i = 0; i < map._onChangeHandlers.length; i++) {
                    try { map._onChangeHandlers[i](id, entity); } catch (e) { this.emit('error', e); }
                }
                for (let i = 0; i < this._onCollectionChangeHandlers.length; i++) {
                    try { this._onCollectionChangeHandlers[i](id, entity); } catch (e) { this.emit('error', e); }
                }
            } finally {
                this._isTriggering = previous;
            }
        }

        /** Called when an entity is added to the schema state. */
        onAdd(cb) {
            if (typeof cb === 'function') this._onAddHandlers.push(cb);
            return this;
        }

        /** Called when an entity is removed from the schema state. */
        onRemove(cb) {
            if (typeof cb === 'function') this._onRemoveHandlers.push(cb);
            return this;
        }

        /** Called once per changed field: `(entity, fieldName, value)`. */
        onChange(cb) {
            if (typeof cb === 'function') this._onChangeHandlers.push(cb);
            return this;
        }

        /** Called after every applied full/patch frame. */
        onStateChange(cb) {
            if (typeof cb === 'function') this._onStateChangeHandlers.push(cb);
            return this;
        }

        /** Colyseus parity: `room.onError((code, message) => ...)` */
        onError(cb) {
            if (typeof cb !== 'function') return this;
            return this.on('error', (err) => {
                cb((err && err.code) || 0, (err && err.message) || String(err));
            });
        }

        /** Colyseus parity: `room.onReconnect(() => ...)` — fires when the
         *  auto-reconnect lands a fresh session after a drop. */
        onReconnect(cb) {
            if (typeof cb !== 'function') return this;
            return this.on('reconnect', () => cb());
        }

        /** Colyseus parity: `room.onDrop((code, reason) => ...)` — fires when
         *  the connection is lost but a reconnection attempt is coming. */
        onDrop(cb) {
            if (typeof cb !== 'function') return this;
            return this.on('close', (ev) => {
                if (this._autoReconnect && !this._intentionalClose) {
                    cb((ev && ev.code) ?? this._leaveCode ?? 1006, ev && ev.reason);
                }
            });
        }

        /** Colyseus parity: `room.onLeave((code, reason) => ...)` — the code is
         *  the WS close code; a consented `leave()` reports 1000. */
        onLeave(cb) {
            if (typeof cb !== 'function') return this;
            return this.on('close', (ev) => {
                const code = this._leaveCode ?? (ev && ev.code) ?? 1006;
                cb(code, ev && ev.reason);
            });
        }

        /** Colyseus parity: `room.removeAllListeners()` — drops every handler
         *  this room accumulated (messages, state, close, error). */
        removeAllListeners() {
            this._messageHandlers.clear();
            this.listeners.clear();
            this._onAddHandlers.length = 0;
            this._onRemoveHandlers.length = 0;
            this._onChangeHandlers.length = 0;
            this._onCollectionChangeHandlers.length = 0;
            this._onStateChangeHandlers.length = 0;
            return this;
        }

        _emitStateChange() {
            for (let i = 0; i < this._onStateChangeHandlers.length; i++) {
                try { this._onStateChangeHandlers[i](this.state); } catch (e) { this.emit('error', e); }
            }
        }

        /**
         * Start the optional join watchdog (`joinTimeout` option).
         *
         * Without a deadline a client whose first full snapshot never arrives
         * just sits there: the socket is open, no error fires, and the game
         * shows an empty world forever. 0 disables it, which stays the
         * default so existing clients keep their behaviour.
         */
        _armJoinWatchdog() {
            this._clearJoinWatchdog();
            if (!this._joinTimeoutMs) return;
            this._joinTimer = setTimeout(() => {
                this._joinTimer = null;
                if (this._schemaReady) return;
                const message = `[CYG SDK] no full schema snapshot within ${this._joinTimeoutMs} ms`;
                this._schemaError = message;
                this.emit('schemaerror', message);
                try { this.close(4000, 'join timeout'); } catch (_) { /* already closing */ }
            }, this._joinTimeoutMs);
            // Do not hold a Node process (or a test runner) open for the timer.
            if (this._joinTimer && typeof this._joinTimer.unref === 'function') {
                this._joinTimer.unref();
            }
        }

        _clearJoinWatchdog() {
            if (this._joinTimer) {
                clearTimeout(this._joinTimer);
                this._joinTimer = null;
            }
        }

        _schemaDefaultValue(field) {
            if (field.optional) return null;
            if (field.type === 'string') return '';
            if (field.type === 'array') return [];
            if (field.type === 'bool') return false;
            return 0;
        }

        _schemaValueEquals(a, b) {
            if (Object.is(a, b)) return true;
            if (Array.isArray(a) && Array.isArray(b)) {
                if (a.length !== b.length) return false;
                for (let i = 0; i < a.length; i++) {
                    if (!Object.is(a[i], b[i])) return false;
                }
                return true;
            }
            return false;
        }

        _ackSchema() {
            // Every full keyframe is acknowledged, not just the first: a forced
            // full (patch-rate recovery, reconnect) needs its own baseline ack
            // before the host resumes field patches.
            this._schemaAcked = true;
            this.send('__ox_schema_ready');
        }

        // --- schema decode (cyg-core-rs wasm) --------------------------------
        // The whole OXS2 decoder used to live here (~450 lines: _readSchema
        // Element/_readSchemaField/_decodeEntity/_applySchema/_applySchema
        // Frame/_applyCollectionSection/_applyWorldSection). It now lives ONLY
        // in crates/cyg-core-rs — the same Rust core backs the Unity, Godot,
        // Unreal and web (cyg-core-web.mjs) SDKs. cyg.js feeds every raw frame
        // to the wasm, then re-diffs the core's state to hydrate `room.state`
        // and fire the Colyseus-style event callbacks.

        /**
         * Apply one schema delta through the wasm core and hydrate the JS
         * state mirror. `payload` still carries the `[0xA5 "delta"]` prefix —
         * the core unwraps the envelope itself.
         */
        _applySchemaViaCore(payload) {
            if (!this._core) {
                // wasm still loading (async): buffer; the server keeps
                // re-sending the full snapshot until we ack, so nothing is
                // lost even if the queue overflows.
                if (this._pendingSchema.length < 512) this._pendingSchema.push(payload);
                return;
            }
            // Frame layout after the [0xA5 "delta"] prefix: [id i64]["OXS2"]
            // [kind][...]. The prefix is always present on the wire, but the
            // fragment-reassembled lane can hand us a body without it.
            let hasPrefix = (payload[0] === 0xA5 && payload[1] === 0x64 && payload[2] === 0x65)
                || (payload[0] === 0x11 && payload[1] === 0xA5 && payload[2] === 0x64 && payload[3] === 0x65);
            if (!hasPrefix) {
                const prefixed = new Uint8Array(payload.length + 6);
                prefixed.set([0xA5, 0x64, 0x65, 0x6C, 0x74, 0x61]);
                prefixed.set(payload, 6);
                payload = prefixed;
            }
            let kind = 0;
            // After the ensure-prefix step the OXS2 magic always sits at 14.
            const oxs2 = payload[0] === 0x11 ? 15 : 14;
            if (payload.length > oxs2 + 5 && payload[oxs2] === SCHEMA_MAGIC[0]
                && payload[oxs2 + 1] === SCHEMA_MAGIC[1] && payload[oxs2 + 2] === SCHEMA_MAGIC[2]
                && payload[oxs2 + 3] === SCHEMA_MAGIC[3]) {
                kind = payload[oxs2 + 4];
            }
            const n = this._core.apply(this._coreHandle, payload);
            if (n < 0) {
                const message = '[CYG SDK] bad schema frame (rejected by cyg-core)';
                this._schemaError = message;
                this.emit('schemaerror', message);
                return;
            }
            if (kind === 5 || kind === 7) { // packed full: single collection or world
                // Every full keyframe is acknowledged, not just the first: a
                // forced full (patch-rate recovery, reconnect) needs its own
                // baseline ack before the host resumes field patches.
                this._schemaReady = true;
                this._clearJoinWatchdog();
                this._ackSchema();
            }
            try {
                this._hydrateFromCore();
            } catch (e) {
                const message = `[CYG SDK] schema hydrate failed: ${e && e.message ? e.message : e}`;
                this._schemaError = message;
                this.emit('schemaerror', message);
                return;
            }
            this.emit('schema', { kind, state: this.state });
            this._emitStateChange();
        }

        /** Re-diff the core's decoded state into the JS maps/entities. */
        _hydrateFromCore() {
            const core = this._core;
            const handle = this._coreHandle;
            const order = this._collections.slice().sort(
                (a, b) => this._nestedDepth(b.name) - this._nestedDepth(a.name));
            for (const entry of order) {
                this._hydrateCollection(this._core, handle, entry.id, entry.map, entry.schema);
            }
            this._rebuildNested();
            const scalars = this._rootScalars;
            if (scalars) {
                for (let f = 0; f < scalars.fields.length; f++) {
                    const field = scalars.fields[f];
                    const value = this._readRootScalar(core, handle, f, field);
                    const previous = this._rootValues[field.name];
                    if (this._schemaValueEquals(previous, value)) continue;
                    this._rootValues[field.name] = value;
                    this._fireFieldChange(this.state, field.name, value, previous);
                }
            }
        }

        _nestedDepth(name) {
            let depth = 0;
            let current = name;
            for (;;) {
                const nested = (this._nested || []).find(n => n.collection === current);
                if (!nested) return depth;
                depth++;
                current = nested.parent;
            }
        }

        _fillNestedEntity(entity, collectionName) {
            for (const nested of this._nested || []) {
                if (nested.parent !== collectionName) continue;
                const child = this._collections.find(entry => entry.name === nested.collection);
                if (!child) continue;
                let array = entity[nested.field];
                if (!Array.isArray(array)) entity[nested.field] = array = [];
                array.length = 0;
                for (const [id, value] of child.map) {
                    if ((id >>> 16) === entity.id) array[id & 0xffff] = value;
                }
                entity[nested.field] = array;
            }
        }

        /** Fold child-collection rows back into `parent.items` arrays. The
         *  child id packs `(owner << 16) | slot`, so grouping needs no hidden
         *  wire fields; arrays are mutated in place to keep references stable. */
        _rebuildNested() {
            if (!this._nested || this._nested.length === 0) return;
            const byName = new Map(this._collections.map(entry => [entry.name, entry]));
            for (const nested of this._nested) {
                const parent = byName.get(nested.parent);
                const child = byName.get(nested.collection);
                if (!parent || !child) continue;
                for (const entity of parent.map.values()) {
                    const array = entity[nested.field];
                    if (Array.isArray(array)) array.length = 0;
                    else entity[nested.field] = [];
                }
                for (const [id, entity] of child.map) {
                    const owner = entity;
                    const slot = id & 0xffff;
                    const parentEntity = parent.map.get(id >>> 16);
                    if (!parentEntity) continue;
                    const array = parentEntity[nested.field];
                    array[slot] = owner;
                }
            }
        }

        _hydrateCollection(core, handle, ci, map, schema) {
            const count = core.entityCountIn(handle, ci);
            const incoming = new Map();
            for (let i = 0; i < count; i++) {
                const id = core.entityIdAtIn(handle, ci, i);
                if (id === 0xFFFFFFFF) continue;
                incoming.set(id, this._readCoreEntity(core, handle, ci, id, schema));
            }
            for (const [id, entity] of Array.from(map)) {
                if (!incoming.has(id)) {
                    map.delete(id);
                    this._fireRemove(map, entity, id);
                }
            }
            for (const [id, entity] of incoming) {
                const existing = map.get(id);
                if (existing === undefined) {
                    this._fillNestedEntity(entity, this._collections[ci] ? this._collections[ci].name : '');
                    map.set(id, entity);
                    this._trackEntity(entity);
                    this._fireAdd(map, entity, id);
                    continue;
                }
                let changed = false;
                for (const field of schema.fields) {
                    const value = entity[field.name];
                    const previous = existing[field.name];
                    if (this._schemaValueEquals(previous, value)) continue;
                    existing[field.name] = value;
                    this._fireFieldChange(existing, field.name, value, previous);
                    changed = true;
                }
                this._fillNestedEntity(existing, this._collections[ci] ? this._collections[ci].name : '');
                if (changed) this._fireUpdate(map, existing, id);
            }
            map._ready = true;
        }

        /** One typed entity object from the core's decoded row. */
        _readCoreEntity(core, handle, ci, id, schema) {
            const entity = { id };
            for (let i = 0; i < schema.fields.length; i++) {
                const field = schema.fields[i];
                entity[field.name] = this._readCoreField(core, handle, ci, id, field, i);
            }
            this._defineNestedViews(entity, schema);
            return entity;
        }

        /** Group flattened child fields (`stats__kills`) behind `entity.stats`.
         *  The getters read the live flat fields, so per-tick hydration updates
         *  the nested view without allocating a second object. */
        _defineNestedViews(entity, schema) {
            const paths = new Map();
            for (const field of schema.fields) {
                if (!field.nested) continue;
                if (!paths.has(field.nested)) paths.set(field.nested, []);
                paths.get(field.nested).push(field);
            }
            for (const [path, fields] of paths) {
                if (Object.prototype.hasOwnProperty.call(entity, path)) continue;
                const view = {};
                for (const field of fields) {
                    const child = field.name.startsWith(`${path}__`)
                        ? field.name.slice(path.length + 2)
                        : field.name;
                    Object.defineProperty(view, child, {
                        enumerable: true,
                        configurable: true,
                        get: () => entity[field.name],
                        set: (value) => { entity[field.name] = value; }
                    });
                }
                Object.defineProperty(entity, path, {
                    value: view,
                    enumerable: true,
                    configurable: true
                });
            }
        }

        _readCoreField(core, handle, ci, id, field, index) {
            if (field.optional) {
                const value = this._readCoreFieldRequired(core, handle, ci, id, field, index, true);
                return value === undefined ? null : value;
            }
            const value = this._readCoreFieldRequired(core, handle, ci, id, field, index, false);
            return value === undefined ? this._schemaDefaultValue(field) : value;
        }

        _readCoreFieldRequired(core, handle, ci, id, field, index, detectAbsent) {
            // index-based read (schema field order == value order in the core)
            switch (field.type) {
                case 'string': {
                    const s = core.getStringIn(handle, ci, id, index);
                    if (s === null) return detectAbsent ? undefined : '';
                    return s;
                }
                case 'bool': {
                    const b = core.getBoolIn(handle, ci, id, index);
                    if (b < 0) return detectAbsent ? undefined : false;
                    return b === 1;
                }
                case 'i64': {
                    if (ci === 0) {
                        const v = core.getI64(handle, id, index);
                        if (v === 0 && detectAbsent) return undefined;
                        return Number(v);
                    }
                    const v = core.getF64In(handle, ci, id, index);
                    if (Number.isNaN(v)) return detectAbsent ? undefined : 0;
                    return v;
                }
                case 'array':
                    // Arrays are decoded in the core but the C ABI does not
                    // expose element reads yet; hydrate as empty.
                    return [];
                default: {
                    const v = core.getF64In(handle, ci, id, index);
                    if (Number.isNaN(v)) return detectAbsent ? undefined : 0;
                    return v;
                }
            }
        }

        _readRootScalar(core, handle, index, field) {
            switch (field.type) {
                case 'string': {
                    const s = core.getRootString(handle, index);
                    return s === null ? this._schemaDefaultValue(field) : s;
                }
                case 'bool': {
                    const b = core.getRootBool(handle, index);
                    return b < 0 ? this._schemaDefaultValue(field) : b === 1;
                }
                case 'array':
                    return [];
                default: {
                    const v = core.getRootF64(handle, index);
                    return Number.isNaN(v) ? this._schemaDefaultValue(field) : v;
                }
            }
        }

        _maybeAutoReconnect(ev) {
            if (this._intentionalClose || !this._autoReconnect || !this._hasConnected) return;
            if (!this.reconnectionToken || !this._client) return;
            // Server close codes that mean the seat can never resume.
            if (ev && (ev.code === 4001 || ev.code === 4002 || ev.code === 4003)) return;
            if (this._reconnectAttempts >= this._maxReconnectAttempts) {
                this.emit('reconnectfailed');
                return;
            }
            const attempt = ++this._reconnectAttempts;
            const delay = Math.min(500 * Math.pow(2, attempt - 1), 8000);
            this.emit('reconnecting', attempt, delay);
            this._reconnectTimer = setTimeout(async () => {
                this._reconnectTimer = null;
                try {
                    const seat = await this._client._reserve(
                        'reconnect',
                        this.roomName,
                        { reconnectionToken: this.reconnectionToken }
                    );
                    this.reconnectionToken = seat.reconnectionToken || this.reconnectionToken;
                    this._seatSessionId = seat.sessionId || this._seatSessionId;
                    this.seatSessionId = this._seatSessionId;
                    // The fresh seat may live on a different room/process/port
                    // than the one we dialed — rebuild the endpoint the same way
                    // _connectSeat does, or the resume dials a stale address.
                    const roomPath = seat.name || seat.room_id || this.roomName;
                    this.endpoint = this._client._wsBase(seat);
                    this.roomName = roomPath;
                    this._schemaReady = false;
                    this._schemaAcked = false;
                    // Keep the last known state until the resume full snapshot
                    // arrives; _applySchema reconciles removals and changes, so
                    // callers still get onRemove/listen events after a resume.
                    await this.connect();
                    this.emit('reconnected');
                } catch (e) {
                    this.emit('reconnecterror', e);
                    this._maybeAutoReconnect(null);
                }
            }, delay);
        }

        /**
         * Send a typed app message to the server.
         *
         * `data` may be a Uint8Array/ArrayBuffer (binary), a string (UTF-8), or
         * a plain object (JSON). The server delivers the same type + payload to
         * `Room.onMessage(type, ...)`; it is never sent back to other clients
         * unless the room explicitly calls `broadcast`.
         */
        send(type, data = null) {
            if (this.serverRtcActive && this.serverChReliable &&
                this.serverChReliable.readyState === 'open') {
                const frame = this._encodeAppMessage(type, data);
                if (frame) {
                    this.bytesOut += frame.byteLength;
                    try { this.serverChReliable.send(frame); return true; } catch (e) { /* closing */ }
                }
            }
            if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return false;

            const typeBytes = _encoder ? _encoder.encode(String(type)) : null;
            if (!typeBytes || typeBytes.length === 0 || typeBytes.length > APP_MESSAGE_MAX_TYPE) {
                throw new Error('[CYG SDK] message type must be 1..255 UTF-8 bytes');
            }

            let body;
            if (data == null) {
                body = new Uint8Array(0);
            } else if (typeof data === 'string') {
                body = _encoder ? _encoder.encode(data) : new Uint8Array(0);
            } else if (data instanceof Uint8Array) {
                body = data;
            } else if (data instanceof ArrayBuffer) {
                body = new Uint8Array(data);
            } else if (ArrayBuffer.isView(data)) {
                body = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
            } else {
                body = _encoder ? _encoder.encode(JSON.stringify(data)) : new Uint8Array(0);
            }

            const frame = new Uint8Array(2 + typeBytes.length + body.length);
            frame[0] = AppOp.MESSAGE;
            frame[1] = typeBytes.length;
            frame.set(typeBytes, 2);
            frame.set(body, 2 + typeBytes.length);
            this.bytesOut += frame.byteLength;
            this.ws.send(frame);
            return true;
        }

        /** Build one typed app-message frame without choosing a transport. */
        _encodeAppMessage(type, data = null) {
            const typeBytes = _encoder ? _encoder.encode(String(type)) : null;
            if (!typeBytes || typeBytes.length === 0 || typeBytes.length > APP_MESSAGE_MAX_TYPE) {
                throw new Error('[CYG SDK] message type must be 1..255 UTF-8 bytes');
            }
            let body;
            if (data == null) {
                body = new Uint8Array(0);
            } else if (typeof data === 'string') {
                body = _encoder ? _encoder.encode(data) : new Uint8Array(0);
            } else if (data instanceof Uint8Array) {
                body = data;
            } else if (data instanceof ArrayBuffer) {
                body = new Uint8Array(data);
            } else if (ArrayBuffer.isView(data)) {
                body = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
            } else {
                body = _encoder ? _encoder.encode(JSON.stringify(data)) : new Uint8Array(0);
            }
            const frame = new Uint8Array(2 + typeBytes.length + body.length);
            frame[0] = AppOp.MESSAGE;
            frame[1] = typeBytes.length;
            frame.set(typeBytes, 2);
            frame.set(body, 2 + typeBytes.length);
            return frame;
        }

        /** Colyseus parity: fire-and-forget over the unreliable lane. Returns
         *  false when no unreliable path is open (the delta lane drops stale
         *  frames by design — the caller re-sends or re-thinks). */
        /** Input batching: N 20-byte samples in one unreliable packet
       *  ([0x26][count][pad][seq u16 LE][20B]×N) — 75% header overhead cut on
       *  lossy/mobile paths. A batch over the server's 64-sample cap is refused
       *  whole, so keep N ≤ 64. samples: Array of { seq, payload: Uint8Array(20) }. */
      sendInputBatch(samples) {
        if (!this._inputBatchCh && this.serverChUnreliable) { /* noop shim */ }
        const ch = this.serverChUnreliable || this.serverCh;
        if (!ch || ch.readyState !== 'open') return false;
        const count = samples.length;
        // Server cap is 64 samples and count is one byte — refuse oversized
        // (or empty) batches whole, exactly like the host does.
        if (count < 1 || count > 64) return false;
        const pkt = new Uint8Array(3 + count * 22);
        pkt[0] = 0x26;
        pkt[1] = count;
        let off = 3;
        for (let i = 0; i < count; i++) {
          const seq = samples[i].seq & 0xffff;
          pkt[off] = seq & 0xff;
          pkt[off + 1] = (seq >> 8) & 0xff;
          pkt.set(samples[i].payload.subarray(0, 20), off + 2);
          off += 22;
        }
        try { ch.send(pkt); return true; } catch (e) { return false; }
      }

      sendUnreliable(type, data = null) {
            if (this.serverRtcActive && this.serverChUnreliable &&
                this.serverChUnreliable.readyState === 'open') {
                const frame = this._encodeAppMessage(type, data);
                this.bytesOut += frame.byteLength;
                try { this.serverChUnreliable.send(frame); return true; } catch (e) { /* closing */ }
            }
            return false;
        }

        /** Familiar `sendBytes(type, bytes)` name, now using the typed envelope. */
        sendBytes(type, bytes) {
            return this.send(type, bytes);
        }

        /** Convenience helper for JSON payloads. */
        sendJSON(type, value) {
            return this.send(type, JSON.stringify(value));
        }

        /** Escape hatch for an already-framed/raw transport message. */
        sendRaw(bytes) {
            if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                const length = bytes && (bytes.byteLength ?? bytes.length) || 0;
                this.bytesOut += length;
                this.ws.send(bytes);
                return true;
            }
            return false;
        }

        /**
         * Register a handler for a typed server message.
         * `room.onMessage('discard', (payload) => { ... })`
         */
        onMessage(type, cb) {
            if (!type || typeof cb !== 'function') return this;
            if (!this._messageHandlers.has(type)) this._messageHandlers.set(type, []);
            this._messageHandlers.get(type).push(cb);
            return this;
        }

        /**
         * request/response (RPC-style): reliable lane pe '__reqX' bhejo, room
         * ke reply ('__resX' + same reqId) pe resolve hota hai. Timeout pe
         * reject. Room side iska jawab `onRequest` sugar se deta hai.
         */
        request(type, data = null, timeoutMs = 5000) {
            return new Promise((resolve, reject) => {
                if (!this.isOpen) return reject(new Error('[CYG SDK] room not open'));
                const reliableOpen = this.serverChReliable && this.serverChReliable.readyState === 'open';
                const wsOpen = this.ws && this.ws.readyState === WebSocket.OPEN;
                if (!reliableOpen && !wsOpen) {
                    return reject(new Error(`[CYG SDK] request '${type}' failed: no reliable lane open (DataChannel closed and WebSocket not open)`));
                }
                const reqId = ((this._reqSeq = (this._reqSeq + 1) & 0xFFFF) === 0) ? 1 : this._reqSeq;
                const timer = setTimeout(() => {
                    this._pendingRequests.delete(reqId);
                    reject(new Error(`[CYG SDK] request '${type}' timed out (${timeoutMs}ms)`));
                }, timeoutMs);
                this._pendingRequests.set(reqId, { resolve, reject, timer });

                // Wire: type='__req', payload=[reqId u16][typeLen u8][type][data]
                const typeBytes = _encoder.encode(String(type));
                const dataBytes = data instanceof Uint8Array ? data
                    : (data instanceof ArrayBuffer ? new Uint8Array(data)
                    : (ArrayBuffer.isView(data) ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength)
                    : _encoder.encode(typeof data === 'string' ? data : JSON.stringify(data ?? ''))));
                const frame = new Uint8Array(10 + typeBytes.length + dataBytes.length);
                frame[0] = AppOp.MESSAGE;
                frame[1] = 5; // type = '__req'
                frame[2] = 0x5F; frame[3] = 0x5F; frame[4] = 0x72; frame[5] = 0x65; frame[6] = 0x71;
                frame[7] = (reqId >> 8) & 0xFF;
                frame[8] = reqId & 0xFF;
                frame[9] = typeBytes.length;
                frame.set(typeBytes, 10);
                frame.set(dataBytes, 10 + typeBytes.length);
                this.bytesOut += frame.byteLength;
                const send_ = this.serverChReliable && this.serverChReliable.readyState === 'open'
                    ? (f) => this.serverChReliable.send(f)
                    : (f) => this.ws.send(f);
                try { send_(frame); } catch (e) {
                    clearTimeout(timer);
                    this._pendingRequests.delete(reqId);
                    return reject(e);
                }
            });
        }

        /** Internal: '__reqX' frame dispatch — pending request resolve. */
        _resolveRequest(type, payload) {
            const reqId = (payload[0] << 8) | payload[1];
            const pending = this._pendingRequests.get(reqId);
            if (!pending) return;
            this._pendingRequests.delete(reqId);
            clearTimeout(pending.timer);
            pending.resolve(payload.subarray(2));
        }

        // ---- server-time clock (TimeSync se aligned) ----
        clock = {
            setTimeout: (fn, ms) => setTimeout(fn, Math.max(0, ms - (this.clockOffsetMs || 0))),
            setInterval: (fn, ms) => setInterval(fn, ms),
            clearTimeout: (id) => clearTimeout(id),
            clearInterval: (id) => clearInterval(id),
            now: () => this.serverNow(),
            /** Server's estimated wall time right now (TimeSync line). */
            serverNow: () => this.serverNow(),
            /** Smoothed RTT in ms — Phase D echo pair when available, else ping. */
            smoothedRtt: () => (this._echoRtt >= 0 ? this._echoRtt : this.rtt),
            /** Jitter in ms — RFC 3550 deviation (echo) or ping estimate. */
            jitter: () => (this._echoRtt >= 0 ? this._echoJitter : this.jitter),
        };

        // --- REPLAY RECORDER ---

        startRecording() {
            this.recording = true;
            this.recordStartTime = performance.now();
            this.recordedFrames = [];
            console.log(`[CYG SDK] ⏺️ Match recording started for room "${this.roomName}"`);
        }

        stopRecording() {
            this.recording = false;
            const duration = Math.round(performance.now() - this.recordStartTime);
            console.log(`[CYG SDK] ⏹️ Recording captured ${this.recordedFrames.length} frames (${duration}ms)`);
            return {
                format: "cyg-replay-1",
                room: this.roomName,
                recordedAt: new Date().toISOString(),
                durationMs: duration,
                totalFrames: this.recordedFrames.length,
                frames: this.recordedFrames
            };
        }

        downloadReplay(filename = "match.oxr") {
            const data = this.stopRecording();
            const jsonStr = JSON.stringify(data);
            const blob = new Blob([jsonStr], { type: "application/json" });
            const url = URL.createObjectURL(blob);
            const a = document.createElement("a");
            a.href = url;
            a.download = filename;
            document.body.appendChild(a);
            a.click();
            document.body.removeChild(a);
            URL.revokeObjectURL(url);
            return data;
        }

        on(event, cb) {
            if (!this.listeners.has(event)) this.listeners.set(event, []);
            this.listeners.get(event).push(cb);
            return this;
        }

        /** Remove one handler, or every handler for `event` when `cb` is omitted. */
        off(event, cb) {
            const list = this.listeners.get(event);
            if (!list) return this;
            if (typeof cb !== 'function') {
                this.listeners.delete(event);
                return this;
            }
            const index = list.indexOf(cb);
            if (index >= 0) list.splice(index, 1);
            if (list.length === 0) this.listeners.delete(event);
            return this;
        }

        emit(event, ...args) {
            const list = this.listeners.get(event);
            if (list) {
                for (let i = 0; i < list.length; i++) {
                    list[i](...args);
                }
            }
        }
    }

    /**
     * WebRTC voice mesh for one room.
     *
     * Every remote peer gets its own `RTCPeerConnection` carrying the local mic
     * track, so Opus media flows browser-to-browser and never touches the game
     * server. The server only relays negotiation frames (see
     * `CYGRoom.sendSignal`), so it stays authoritative about who is in the
     * room without ever holding audio.
     *
     * Two rules keep the mesh from negotiating with itself:
     *  - the peer with the LOWER player id always creates the offer, so a pair can
     *    never produce two competing offers;
     *  - the sender id on every frame is the one the server stamped from its own
     *    session map, never the one the remote browser claimed.
     *
     * When a pair cannot connect (symmetric NATs with no TURN configured), the peer
     * falls back to `relay` mode and the game's existing `sendVoice()` path keeps
     * working. `isP2P(id)` says which transport carries a speaker, so the caller
     * never plays the same voice twice.
     */
    class CYGVoiceMesh {
        constructor(room, options = {}) {
            this.room = room;
            this.options = options;
            this.selfId = (options.selfId != null) ? Number(options.selfId) : null;
            this.iceServers = Array.isArray(options.iceServers) ? options.iceServers.slice() : [];
            this.peerTimeoutMs = options.peerTimeoutMs || 12000;
            // Two DataChannels per peer carry game frames on the same connection
            // that carries the voice track, so a game gets UDP behaviour without
            // a second negotiation. Set false for voice-only clients.
            this.dataChannels = options.dataChannels !== false;

            this.micStream = null;
            this.micTrack = null;
            this.ownsMic = false;
            this.enabled = false;
            this.disposed = false;

            // peerId -> { pc, mode, pendingIce, remote, nodes, deadline, initiated }
            this.peers = new Map();
            // ids learned about before the mic or the player id was available
            this._unwired = [];
            this._serverMuted = new Set();
            this.audioContext = options.audioContext || null;
            this.listeners = new Map();
        }

        static isSupported() {
            return (typeof RTCPeerConnection !== 'undefined');
        }

        on(event, cb) {
            if (!this.listeners.has(event)) this.listeners.set(event, []);
            this.listeners.get(event).push(cb);
            return this;
        }

        emit(event, ...args) {
            const list = this.listeners.get(event);
            if (list) for (let i = 0; i < list.length; i++) list[i](...args);
        }

        /**
         * Start WebRTC negotiation. Automatically connects RTCDataChannels over UDP.
         * If a MediaStream is supplied, audio is also attached; otherwise data channels
         * connect immediately and mic can be attached later via setMicStream.
         */
        async enable(stream = null) {
            if (this.disposed) throw new Error('voice mesh is disposed');
            if (this.enabled) return;

            if (stream) {
                this.micStream = stream;
                this.ownsMic = false;
                this.micTrack = this.micStream.getAudioTracks()[0] || null;
            }

            this.enabled = true;

            const sent = this.room.sendSignal(SignalKind.JOIN, SIGNAL_BROADCAST, {
                version: SIGNAL_VERSION,
                capabilities: 'webrtc,datachannel' + (this.micTrack ? ',opus' : '')
            });
            if (!sent) {
                this.enabled = false;
                throw new Error('room socket is not open');
            }
            this._flushUnwired();
        }

        setMicStream(stream) {
            this.micStream = stream;
            this.micTrack = stream ? (stream.getAudioTracks()[0] || null) : null;
            if (!this.enabled) {
                this.enable(stream).catch(() => {});
                return;
            }
            for (const [peerId, entry] of this.peers) {
                if (entry.pc && this.micTrack) {
                    try {
                        const senders = entry.pc.getSenders ? entry.pc.getSenders() : [];
                        const audioSender = senders.find(s => (s.track && s.track.kind === 'audio') || (s.dtmf !== undefined));
                        if (audioSender && audioSender.replaceTrack) {
                            audioSender.replaceTrack(this.micTrack);
                        } else {
                            entry.pc.addTrack(this.micTrack, this.micStream);
                        }
                    } catch (e) {
                        console.warn('[voice] addTrack/replaceTrack error:', e);
                    }
                }
            }
        }

        /**
         * Tell the mesh which player id this client owns. The server assigns ids from
         * the game state, so the app reports it once the first delta lands; peers that
         * turned up before that are wired here.
         */
        setSelfId(id) {
            const numeric = Number(id);
            if (!Number.isFinite(numeric) || numeric === this.selfId) return;
            this.selfId = numeric;
            this._flushUnwired();
        }

        _flushUnwired() {
            if (this.selfId == null || !this.enabled || this._unwired.length === 0) return;
            const queued = this._unwired.splice(0);
            for (const id of queued) this._ensurePeer(id);
        }

        /**
         * Get (or create) the connection for a peer. `silent` creates the entry
         * without offering, which is what an inbound negotiation needs.
         */
        _ensurePeer(peerId, silent) {
            if (this.disposed) return null;
            const id = Number(peerId);
            if (!Number.isFinite(id) || id === this.selfId) return null;
            if (this.peers.has(id)) return this.peers.get(id);
            if (this.selfId == null || !this.enabled) {
                if (this._unwired.indexOf(id) < 0) this._unwired.push(id);
                return null;
            }

            const initiates = this.selfId < id;
            const entry = {
                pc: null, mode: 'connecting', pendingIce: [], remote: null,
                nodes: null, deadline: null, initiated: false,
                chUnreliable: null, chReliable: null,
                lastInput: null, lastState: null
            };
            this.peers.set(id, entry);
            try {
                entry.pc = this._createPeerConnection(id, entry);
            } catch (e) {
                this.peers.delete(id);
                this.emit('error', { peer: id, reason: String(e) });
                return null;
            }
            entry.deadline = setTimeout(() => this._teardown(id, 'failed'), this.peerTimeoutMs);
            this.emit('peer', id, entry.mode);
            if (initiates && !silent) this._offer(id);
            return entry;
        }

        _createPeerConnection(peerId, entry) {
            const pc = new RTCPeerConnection({
                iceServers: this.iceServers,
                bundlePolicy: 'max-bundle',
                rtcpMuxPolicy: 'require'
            });
            if (this.micTrack) {
                pc.addTrack(this.micTrack, this.micStream);
            } else if (pc.addTransceiver) {
                try {
                    pc.addTransceiver('audio', { direction: 'sendrecv' });
                } catch (e) {}
            }

            pc.onicecandidate = (e) => {
                if (e.candidate) {
                    this.room.sendSignal(SignalKind.ICE, peerId, { candidate: e.candidate });
                }
            };
            pc.ontrack = (e) => this._attachRemote(peerId, e.streams && e.streams[0]);
            pc.onconnectionstatechange = () => this._onConnectionState(peerId);
            // The answering side receives the channels the offerer declares.
            pc.ondatachannel = (e) => this._bindChannel(peerId, e.channel);
            if (this.dataChannels && this.selfId < peerId) this._openChannels(peerId, pc, entry);
            return pc;
        }

        /**
         * Unreliable for anything the next frame replaces, reliable for anything
         * that must not vanish. `maxRetransmits: 0` is what removes TCP-style
         * head-of-line blocking from the fast path.
         */
        _openChannels(peerId, pc, entry) {
            try {
                entry.chUnreliable = pc.createDataChannel('game-unreliable', {
                    ordered: false, maxRetransmits: 0, protocol: 'cyg-mesh-1'
                });
                entry.chReliable = pc.createDataChannel('game-reliable', {
                    ordered: true, protocol: 'cyg-mesh-1'
                });
            } catch (e) {
                this.emit('error', { peer: peerId, reason: 'datachannel: ' + String(e) });
            }
            if (entry.chUnreliable) this._bindChannel(peerId, entry.chUnreliable);
            if (entry.chReliable) this._bindChannel(peerId, entry.chReliable);
        }

        _bindChannel(peerId, channel) {
            if (!channel) return;
            const id = Number(peerId);
            const entry = this.peers.get(id);
            if (!entry) {
                try { channel.close(); } catch (e) { /* already gone */ }
                return;
            }
            const unreliable = String(channel.label).indexOf('unreliable') >= 0;
            if (unreliable) entry.chUnreliable = channel; else entry.chReliable = channel;
            channel.onopen = () => this.emit('channel', id, unreliable ? 'unreliable' : 'reliable');
            channel.onclose = () => {
                if (unreliable) entry.chUnreliable = null; else entry.chReliable = null;
            };
            channel.onmessage = (ev) => {
                const bytes = ev.data instanceof ArrayBuffer
                    ? new Uint8Array(ev.data)
                    : new Uint8Array(ev.data.buffer, ev.data.byteOffset || 0, ev.data.byteLength);
                this._onMeshFrame(id, bytes);
            };
        }

        /**
         * Inbound frame, with the mesh's own staleness rule applied: a retransmit
         * that arrives after the frame it replaces is dropped rather than replayed.
         */
        _onMeshFrame(peerId, bytes) {
            const frame = decodeMeshFrame(bytes);
            if (!frame) { this.emit('mesherror', peerId, bytes); return; }
            const entry = this.peers.get(Number(peerId));
            if (frame.op === MeshOp.INPUT) {
                if (entry && entry.lastInput != null && !isNewerSeq(entry.lastInput, frame.seq)) return;
                if (entry) entry.lastInput = frame.seq;
            } else if (frame.op === MeshOp.STATE) {
                if (entry && entry.lastState != null && !isNewerSeq(entry.lastState, frame.tick)) return;
                if (entry) entry.lastState = frame.tick;
            }
            this.emit('mesh', Number(peerId), frame);
            this.emit(
                frame.op === MeshOp.INPUT ? 'input'
                    : frame.op === MeshOp.STATE ? 'state'
                        : frame.op === MeshOp.EVENT ? 'event' : 'voicedata',
                Number(peerId), frame);
        }

        async _offer(peerId) {
            const entry = this.peers.get(peerId);
            if (!entry || entry.initiated || !entry.pc) return;
            entry.initiated = true;
            try {
                const offer = await entry.pc.createOffer();
                await entry.pc.setLocalDescription(offer);
                this.room.sendSignal(SignalKind.OFFER, peerId, { sdp: entry.pc.localDescription });
            } catch (e) {
                this._teardown(peerId, 'failed');
            }
        }

        _onSignal(frame) {
            if (this.disposed || !frame || !frame.body) return;
            const b = frame.body;

            switch (frame.kind) {
                case SignalKind.CONFIG:
                    // The server knows our authoritative entity id before replicated
                    // state reaches us, and the initiator rule needs it up front.
                    if (b.self != null) this.setSelfId(Number(b.self));
                    // The server's ICE list wins when the app did not configure one:
                    // STUN/TURN is an operator decision, not a per-page constant.
                    if (Array.isArray(b.iceServers) && this.iceServers.length === 0) {
                        this.iceServers = b.iceServers;
                    }
                    if (Array.isArray(b.peers)) {
                        for (const id of b.peers) this._ensurePeer(id);
                    }
                    this.emit('config', b);
                    break;
                case SignalKind.PEER_JOINED:
                    this._ensurePeer(Array.isArray(b) ? b[0] : b.peer);
                    break;
                case SignalKind.PEER_LEFT:
                    this._teardown(Array.isArray(b) ? b[0] : b.peer, 'left');
                    break;
                case SignalKind.MUTED:
                    this._applyServerMute(Number(b[0]), !!b[1]);
                    break;
                case SignalKind.OFFER:
                    this._acceptOffer(frame.from, b.sdp);
                    break;
                case SignalKind.ANSWER:
                    this._acceptAnswer(frame.from, b.sdp);
                    break;
                case SignalKind.ICE:
                    this._addCandidate(frame.from, b.candidate);
                    break;
                case SignalKind.ERROR:
                    this.emit('error', { reason: b.reason || 'rejected' });
                    break;
                default:
                    this.emit('signal', frame);
            }
        }

        /**
         * An offer from a peer we did not know about is legitimate: the roster
         * announcement can arrive after the offer, so the entry is created silently.
         */
        async _acceptOffer(peerId, sdp) {
            const entry = this._ensurePeer(peerId, true);
            if (!entry || !entry.pc || !sdp) return;
            try {
                await entry.pc.setRemoteDescription(new RTCSessionDescription(sdp));
                this._flushPendingIce(entry);
                const answer = await entry.pc.createAnswer();
                await entry.pc.setLocalDescription(answer);
                this.room.sendSignal(SignalKind.ANSWER, peerId, { sdp: entry.pc.localDescription });
            } catch (e) {
                this._teardown(peerId, 'failed');
            }
        }

        async _acceptAnswer(peerId, sdp) {
            const entry = this.peers.get(Number(peerId));
            if (!entry || !entry.pc || !sdp) return;
            try {
                await entry.pc.setRemoteDescription(new RTCSessionDescription(sdp));
                this._flushPendingIce(entry);
            } catch (e) {
                this._teardown(Number(peerId), 'failed');
            }
        }

        async _addCandidate(peerId, candidate) {
            const entry = this._ensurePeer(peerId, true);
            if (!entry || !entry.pc) return;
            const init = candidate ? new RTCIceCandidate(candidate) : null;
            const rd = entry.pc.remoteDescription;
            if (rd && rd.type) {
                try { await entry.pc.addIceCandidate(init); }
                catch (e) { /* a stale candidate is not worth tearing a peer down for */ }
            } else {
                // ICE routinely outruns the SDP answer; hold it until remoteDescription lands.
                entry.pendingIce.push(init);
            }
        }

        _flushPendingIce(entry) {
            const queued = entry.pendingIce.splice(0);
            for (const init of queued) {
                entry.pc.addIceCandidate(init).catch(() => {});
            }
        }

        _onConnectionState(peerId) {
            const entry = this.peers.get(Number(peerId));
            if (!entry || !entry.pc) return;
            const state = entry.pc.connectionState;

            if (state === 'connected' || state === 'completed') {
                if (entry.deadline) { clearTimeout(entry.deadline); entry.deadline = null; }
                if (entry.mode !== 'p2p') {
                    entry.mode = 'p2p';
                    this.emit('mode', Number(peerId), 'p2p');
                }
            } else if (state === 'disconnected') {
                // Only the offerer restarts ICE, otherwise both ends renegotiate.
                if (this.selfId < Number(peerId) && entry.pc.signalingState === 'stable') {
                    entry.pc.restartIce();
                }
            } else if (state === 'failed' || state === 'closed') {
                this._teardown(Number(peerId), 'failed');
            }
        }

        _attachRemote(peerId, stream) {
            const entry = this.peers.get(Number(peerId));
            if (!entry || !stream || entry.remote === stream) return;
            entry.remote = stream;
            this._buildPlaybackGraph(entry, stream);
            this.emit('stream', Number(peerId), stream);
        }

        /**
         * Remote audio goes through a gain (and pan) node so the game can place a
         * speaker from the world state it already has, instead of waiting for the
         * server to fan a volume out.
         */
        _buildPlaybackGraph(entry, stream) {
            try {
                if (this.audioContext && this.audioContext.createMediaStreamSource) {
                    const source = this.audioContext.createMediaStreamSource(stream);
                    const gain = this.audioContext.createGain();
                    gain.gain.value = 1.0;
                    let tail = gain;
                    if (this.audioContext.createStereoPanner) {
                        const panner = this.audioContext.createStereoPanner();
                        gain.connect(panner);
                        tail = panner;
                        entry.nodes = { source, gain, panner, element: null };
                    } else {
                        entry.nodes = { source, gain, panner: null, element: null };
                    }
                    tail.connect(this.audioContext.destination);
                    return;
                }
            } catch (e) { /* fall through to the plain element */ }

            if (typeof Audio !== 'undefined') {
                const el = new Audio();
                el.srcObject = stream;
                el.autoplay = true;
                el.play().catch(() => {});
                entry.nodes = { source: null, gain: null, panner: null, element: el };
            }
        }

        /**
         * Position one speaker in the mix. `volume` 0..1, `pan` -1..1. The game owns
         * the distance model, because it owns the world.
         */
        setPeerAudio(peerId, volume, pan) {
            const entry = this.peers.get(Number(peerId));
            if (!entry || !entry.nodes) return;
            const muted = this._serverMuted.has(Number(peerId));
            const v = muted ? 0 : Math.max(0, Math.min(1, volume == null ? 1 : volume));
            if (entry.nodes.gain) {
                entry.nodes.gain.gain.value = v;
                if (entry.nodes.panner && Number.isFinite(pan)) {
                    entry.nodes.panner.pan.value = Math.max(-1, Math.min(1, pan));
                }
            } else if (entry.nodes.element) {
                entry.nodes.element.volume = v;
            }
        }

        /**
         * Stop transmitting on the P2P path. A server never sees peer-to-peer media,
         * so an authoritative mute can only silence the relay; this is what actually
         * takes the speaker off the air for direct peers.
         */
        setMuted(muted) {
            if (this.micTrack) this.micTrack.enabled = !muted;
        }

        /** Enforce the server's mute verdict on an inbound peer stream. */
        _applyServerMute(peerId, muted) {
            if (muted) this._serverMuted.add(peerId); else this._serverMuted.delete(peerId);
            this.setPeerAudio(peerId, 1.0, 0.0);
        }

        /** True when this speaker's audio arrives directly, not through the relay. */
        isP2P(peerId) {
            const entry = this.peers.get(Number(peerId));
            return !!entry && entry.mode === 'p2p';
        }

        /** True when the relay path still has to carry this peer. */
        shouldRelay(peerId) {
            return !this.isP2P(peerId);
        }

        peerIds() {
            return Array.from(this.peers.keys());
        }

        // --- MESH GAME DATA (frames defined by engine-webrtc-protocol) ---

        /** True when a peer's data channel is open, not merely peer-connected. */
        isMeshReady(peerId) {
            const entry = this.peers.get(Number(peerId));
            return !!entry && !!entry.chUnreliable && entry.chUnreliable.readyState === 'open';
        }

        /** Write one frame to every peer holding an open channel; returns the count. */
        _broadcast(bytes, reliable) {
            if (!bytes) return 0;
            let sent = 0;
            for (const entry of this.peers.values()) {
                const ch = reliable ? entry.chReliable : entry.chUnreliable;
                if (!ch || ch.readyState !== 'open') continue;
                // A full send queue is not retried: the next frame replaces this one.
                try { ch.send(bytes); sent += 1; } catch (e) { /* ignore */ }
            }
            return sent;
        }

        sendInput(input) { return this._broadcast(encodeInputFrame(input), false); }

        sendState(tick, entities) { return this._broadcast(encodeStateFrame(tick, entities), false); }

        /** Events go out ordered and retransmitted — a shot must not evaporate. */
        sendEvent(event) { return this._broadcast(encodeEventFrame(event), true); }

        /** A VoicePacket buffer, sent to peers exactly as the relay would carry it. */
        sendVoicePacket(packet) { return this._broadcast(packet, false); }

        _teardown(peerId, reason) {
            const id = Number(peerId);
            const entry = this.peers.get(id);
            if (!entry) return;
            if (entry.deadline) clearTimeout(entry.deadline);
            if (entry.pc) {
                entry.pc.onicecandidate = null;
                entry.pc.ontrack = null;
                entry.pc.onconnectionstatechange = null;
                try { entry.pc.close(); } catch (e) {}
            }
            // Channels die with the connection, but their handlers must stop
            // firing before that happens or a closing peer queues frames for us.
            for (const ch of [entry.chUnreliable, entry.chReliable]) {
                if (!ch) continue;
                ch.onopen = null;
                ch.onclose = null;
                ch.onmessage = null;
                try { ch.close(); } catch (e) { /* already closing */ }
            }
            entry.chUnreliable = null;
            entry.chReliable = null;
            if (entry.nodes) {
                if (entry.nodes.source) { try { entry.nodes.source.disconnect(); } catch (e) {} }
                if (entry.nodes.gain) { try { entry.nodes.gain.disconnect(); } catch (e) {} }
                if (entry.nodes.panner) { try { entry.nodes.panner.disconnect(); } catch (e) {} }
                if (entry.nodes.element) { try { entry.nodes.element.pause(); entry.nodes.element.srcObject = null; } catch (e) {} }
            }
            this.peers.delete(id);
            this._serverMuted.delete(id);

            if (reason === 'failed') {
                // Media for this peer now has to come back through the server relay.
                this.emit('mode', id, 'relay');
            }
            this.emit('peerleft', id, reason);
        }

        /** Leave the mesh without dropping the game connection. */
        disable() {
            if (this.enabled) {
                this.room.sendSignal(SignalKind.BYE, SIGNAL_BROADCAST, { version: SIGNAL_VERSION });
                this.enabled = false;
            }
            for (const id of Array.from(this.peers.keys())) this._teardown(id, 'left');
            this._unwired.length = 0;
            this._releaseMic();
        }

        _releaseMic() {
            if (this.ownsMic && this.micStream) {
                for (const t of this.micStream.getTracks()) t.stop();
            }
            this.micStream = null;
            this.micTrack = null;
            this.ownsMic = false;
        }

        dispose() {
            if (this.disposed) return;
            this.disposed = true;
            this.disable();
            this.listeners.clear();
        }
    }

    // --- REPLAY PLAYER (Instant Replay, Kill-Cam, Match Spectating) ---
    class CYGReplayPlayer {
        constructor(replayData) {
            this.data = replayData;
            this.frames = replayData.frames.map(f => ({
                t: f.t,
                bytes: new Uint8Array(f.d)
            }));
            this.durationMs = replayData.durationMs || (this.frames.length > 0 ? this.frames[this.frames.length - 1].t : 0);
            this.currentIndex = 0;
            this.isPlaying = false;
            this.speed = 1.0;
            this.clock = 0;
            this.lastRaf = 0;
            this.onDeltaCallback = null;
            this.onProgressCallback = null;
            this.onEndedCallback = null;
        }

        play(onDelta) {
            if (onDelta) this.onDeltaCallback = onDelta;
            if (this.isPlaying) return;

            if (this.currentIndex >= this.frames.length) {
                this.currentIndex = 0;
                this.clock = 0;
            }

            this.isPlaying = true;
            this.lastRaf = performance.now();

            const scheduleNext = (typeof requestAnimationFrame !== 'undefined')
                ? requestAnimationFrame
                : (cb) => setTimeout(() => cb(performance.now()), 16);

            const loop = (now) => {
                if (!this.isPlaying) return;

                const dt = (now - this.lastRaf) * this.speed;
                this.lastRaf = now;
                this.clock += dt;

                while (this.currentIndex < this.frames.length && this.frames[this.currentIndex].t <= this.clock) {
                    const f = this.frames[this.currentIndex];
                    if (this.onDeltaCallback) {
                        this.onDeltaCallback(f.bytes, this.currentIndex, this.frames.length);
                    }
                    this.currentIndex++;
                }

                if (this.onProgressCallback) {
                    const progress = this.durationMs > 0 ? Math.min(1.0, this.clock / this.durationMs) : 1.0;
                    this.onProgressCallback(progress, this.clock, this.durationMs);
                }

                if (this.currentIndex >= this.frames.length) {
                    this.isPlaying = false;
                    if (this.onEndedCallback) this.onEndedCallback();
                    return;
                }

                scheduleNext(loop);
            };

            scheduleNext(loop);
        }

        pause() {
            this.isPlaying = false;
        }

        seek(progress) {
            const clamped = Math.max(0, Math.min(1.0, progress));
            const targetTime = clamped * this.durationMs;
            this.clock = targetTime;

            let foundIdx = 0;
            for (let i = 0; i < this.frames.length; i++) {
                if (this.frames[i].t <= targetTime) {
                    foundIdx = i;
                } else {
                    break;
                }
            }

            this.currentIndex = foundIdx;
            if (this.frames[foundIdx] && this.onDeltaCallback) {
                this.onDeltaCallback(this.frames[foundIdx].bytes, foundIdx, this.frames.length);
            }

            if (this.onProgressCallback) {
                this.onProgressCallback(clamped, this.clock, this.durationMs);
            }
        }

        setSpeed(s) {
            this.speed = Math.max(0.1, Math.min(5.0, s));
        }

        onEnded(cb) {
            this.onEndedCallback = cb;
            return this;
        }

        onProgress(cb) {
            this.onProgressCallback = cb;
            return this;
        }
    }

    /**
     * GGPO-Style Client Predict & Rollback Engine.
     * Manages client-side inputs, instant zero-latency local prediction,
     * historical frame snapshots, and authoritative state reconciliations.
     */
    /** Shallow key-mirror: copies every own key from `src` into `out`, dropping
     *  stale keys, so reused pose/scratch objects never leak old fields. */
    function copyPose(out, src) {
        for (const k in out) if (!(k in src)) delete out[k];
        for (const k in src) out[k] = src[k];
        return out;
    }

    function median(values) {
        const n = values.length;
        if (n === 0) return NaN;
        const s = values.slice().sort((a, b) => a - b);
        return (n & 1) ? s[(n - 1) >> 1] : (s[n / 2 - 1] + s[n / 2]) * 0.5;
    }

    /**
     * TimeSync — robust server-timeline estimator (Lightyear-style offset model).
     * Records the last `maxSamples` (~64) delta arrivals as (arrivalMs, serverTick)
     * points and fits a line tick = (time - origin) / msPerTick. The slope is the
     * Theil-Sen median-of-consecutive-slopes (clamped to +/-25 % of the game's
     * nominal `tickRate`, so bursts cannot make it run away); the intercept
     * `tickOriginMs` is taken from the LOWER envelope — the smallest arrival-side
     * origin, i.e. the lowest-latency samples — so late arrivals are discarded the
     * way Lightyear discards high-offset frames. Purely passive: fed once per delta,
     * it adds no traffic of its own.
     */
    class CYGTimeSync {
        constructor(maxSamples = 64) {
            this.maxSamples = maxSamples;
            this.nominalTickRate = 128;
            this.tickRate = 128;
            this.msPerTick = 1000 / 128;
            this.tickOriginMs = -1;
            this.minLatencyMs = 0;
            this.synced = false;
            this._pts = [];
        }

        setTickRate(rate) {
            if (Number.isFinite(rate) && rate > 0) this.nominalTickRate = rate;
        }

        addSample(arrivalMs, serverTick) {
            if (!Number.isFinite(arrivalMs) || !Number.isFinite(serverTick)) return;
            const pts = this._pts;
            if (pts.length && serverTick <= pts[pts.length - 1].tick) {
                // Same-tick retransmit refreshes arrival; older tick is ignored.
                if (serverTick === pts[pts.length - 1].tick) pts[pts.length - 1] = { t: arrivalMs, tick: serverTick };
            } else {
                pts.push({ t: arrivalMs, tick: serverTick });
            }
            while (pts.length > this.maxSamples) pts.shift();
            this._fit();
        }

        _fit() {
            const pts = this._pts;
            if (pts.length === 0) return;
            this.msPerTick = 1000 / this.nominalTickRate;
            this.tickRate = this.nominalTickRate;

            if (pts.length >= 2) {
                const slopes = [];
                for (let i = 1; i < pts.length; i++) {
                    const dt = pts[i].t - pts[i - 1].t;
                    const dn = pts[i].tick - pts[i - 1].tick;
                    if (dt > 0 && dn > 0) slopes.push(dt / dn);
                }
                if (slopes.length) {
                    const med = median(slopes);
                    const nominal = 1000 / this.nominalTickRate;
                    this.msPerTick = Math.max(nominal * 0.75, Math.min(nominal * 1.25, med));
                    this.tickRate = 1000 / this.msPerTick;
                }
            }

            let best = Infinity;
            for (const p of pts) {
                const origin = p.t - p.tick * this.msPerTick;
                if (origin < best) best = origin;
            }
            this.tickOriginMs = best;
            this.synced = true;
        }

        tickToTimeMs(tick) {
            if (!this.synced) return tick * this.msPerTick;
            return this.tickOriginMs + tick * this.msPerTick;
        }

        timeToTick(t) {
            if (!this.synced) return 0;
            return (t - this.tickOriginMs) / this.msPerTick;
        }

        clockOffsetMs(nowMs) {
            // serverTime(now) = tick(now) * msPerTick = now - origin, so the offset
            // that aligns client time to server time is simply -tickOriginMs.
            if (!this.synced) return 0;
            return -this.tickOriginMs;
        }

        sampleCount() { return this._pts.length; }

        reset() {
            this._pts.length = 0;
            this.tickOriginMs = -1;
            this.synced = false;
        }
    }

    /**
     * InterpolationBuffer — Lightyear-timeline-inspired snapshot buffer.
     * Push decoded server snapshots; sample() returns the state rendered
     * `delayMs` in the past, smoothly interpolated between the two
     * surrounding snapshots. Absorbs network jitter completely.
     */
    class CYGInterpolationBuffer {
        constructor(lerpFn, delayMs = 100) {
            this.lerpFn = lerpFn || null;
            this.delayMs = delayMs;
            this.snaps = [];
            this.maxSnaps = 32;
        }

        push(state, arrivalTime) {
            this.snaps.push({ t: arrivalTime ?? performance.now(), state });
            if (this.snaps.length > this.maxSnaps) this.snaps.shift();
        }

        /** Interpolated state at `now - delayMs`. Returns null before first snap. */
        sample(now = performance.now()) {
            if (this.snaps.length === 0) return null;
            const target = now - this.delayMs;
            const last = this.snaps[this.snaps.length - 1];
            if (target >= last.t) return last.state;
            const first = this.snaps[0];
            if (target <= first.t) return first.state;

            for (let i = this.snaps.length - 1; i > 0; i--) {
                const b = this.snaps[i], a = this.snaps[i - 1];
                if (target >= a.t) {
                    const span = b.t - a.t || 1;
                    const alpha = (target - a.t) / span;
                    if (this.lerpFn) return this.lerpFn(a.state, b.state, alpha);
                    return b.state;
                }
            }
            return first.state;
        }
    }

    /**
     * InterpolationTimeline — tick-authoritative remote-entity smoother
     * (Lightyear-style client-side interpolation).
     *
     * Snapshots are keyed by the server tick they were produced on. A render
     * clock runs on local time but tracks server ticks: it is initialised from
     * the first snapshot's (tick, arrival) pair, its tick duration is the
     * arrival-weighted EMA of observed tick deltas, and it always renders
     * `interpTicks` (default 2) ticks in the past. Jitter is absorbed by a
     * margin: when the buffer runs long the clock speeds up by
     * `speedupFactor` (1.05), and if the render target falls more than
     * `snapTicks` (10) ticks behind the newest snapshot — a 10+ tick packet-loss
     * gap — the clock snaps forward so entities teleport at most once instead
     * of stalling.
     *
     * Usage:
     *   const tl = new InterpolationTimeline({ interpTicks: 2 });
     *   // on each delta: tl.pushSnapshot(serverTick, { id: {x,y,z,yaw,pitch,...} })
     *   // per frame:    const s = tl.sample(); // lerped snapshot or null
     */
    class CYGInterpolationTimeline {
        constructor(options = {}) {
            this.interpTicks = options.interpTicks ?? 2;
            this.speedupFactor = options.speedupFactor ?? 1.05;
            this.snapTicks = options.snapTicks ?? 10;
            this.maxTicksPerStep = options.maxTicksPerStep ?? 4;
            this.lerpFn = options.lerpFn ?? null;
            // Optional per-field blenders: fieldLerp[k](va, vb, alpha) => number,
            // e.g. shortest-path angle wrapping for yaw/pitch columns.
            this.fieldLerp = options.fieldLerp ?? null;
            // Field indices copied verbatim from the newer snapshot instead of
            // blended: hitpoints, score, state machine — values with no
            // meaningful halfway point.
            this.discrete = options.discrete ?? null;

            this.snaps = [];          // [{ tick, timeMs, entities }], ascending, one per tick
            this.maxSnaps = options.maxSnaps ?? 64;
            this.tickDurationMs = options.tickDurationMs ?? 15.625;
            this._nominalTickMs = this.tickDurationMs; // clamp reference for the estimator
            this.jitterMs = 0;        // observed arrival-interval jitter
            this._lastArrivalMs = -1;
            this._lastDeltaMs = this.tickDurationMs;
            this.renderTick = -Infinity; // fractional clock measured in server ticks
            this._lastSampleMs = -1;
            this._margin = -1;        // current jitter headroom in ticks
            this._targetTicks = this.interpTicks; // interpTicks + _margin last applied
            this._marginChangedMs = 0;

            // Debug counters (read via stats()).
            this._speedupTicks = 0;
            this._snapCount = 0;
        }

        /**
         * Record an authoritative snapshot for `tick`. A retransmit of the same
         * tick replaces the older copy (last write wins); a tick older than
         * anything already buffered is dropped.
         */
        pushSnapshot(tick, entities, arrivalMs) {
            if (!Number.isFinite(tick)) return;
            const now = arrivalMs ?? performance.now();

            const newest = this.snaps.length ? this.snaps[this.snaps.length - 1].tick : -Infinity;
            if (tick < newest) return; // stale snapshot, behind everything we have
            if (tick === newest) {     // retransmit: refresh payload, leave clocks alone
                this.snaps[this.snaps.length - 1] = { tick, timeMs: now, entities };
                return;
            }

            if (this._lastArrivalMs >= 0 && Number.isFinite(newest)) {
                const gap = tick - newest;
                const deltaMs = now - this._lastArrivalMs;
                if (deltaMs >= 0 && gap <= 64) {
                    // One arrival can span `gap` ticks when the server or the
                    // network drops one, so every estimate is normalised to
                    // per-tick units. Without this a dropped tick slows the
                    // clock and visibly changes entity speed.
                    const perTickMs = deltaMs / gap;
                    this.jitterMs = this.jitterMs * 0.9
                        + Math.abs(deltaMs - gap * this._lastDeltaMs) * 0.1;
                    const w = Math.min(0.15, 1 / (this.snaps.length + 2));
                    this._lastDeltaMs = this._lastDeltaMs * (1 - w) + perTickMs * w;
                    // The measured tick length drives the clock, but bursts
                    // (batched retransmits) and stalls must not retune it
                    // wildly: ±25 % of nominal is already enough to track a
                    // server that ticks slightly off the configured rate.
                    this.tickDurationMs = Math.max(this._nominalTickMs * 0.8,
                        Math.min(this._nominalTickMs * 1.25, this._lastDeltaMs));
                }
            }
            this._lastArrivalMs = now;

            this.snaps.push({ tick, timeMs: now, entities });

            if (this.snaps.length === 1) {
                this.renderTick = tick;
                this._lastSampleMs = now;
                this._targetTicks = this.interpTicks + Math.min(6,
                    Math.ceil(this.jitterMs * 4 / this.tickDurationMs));
            }

            while (this.snaps.length > this.maxSnaps) this.snaps.shift();
        }

        /**
         * Advance the clock (call once per frame) and return the lerped snapshot
         * at `newest_tick - interpTicks - jitter_margin` ticks, or null when the
         * timeline has no data yet.
         */
        sample(nowMs) {
            const now = nowMs ?? performance.now();
            if (this.snaps.length === 0) return null;

            if (this.renderTick === -Infinity) {
                this.renderTick = this.snaps[this.snaps.length - 1].tick;
                this._lastSampleMs = now;
            }
            let stepMs = now - this._lastSampleMs;
            if (!(stepMs > 0)) stepMs = 0;
            if (stepMs > this.tickDurationMs * this.maxTicksPerStep) {
                stepMs = this.tickDurationMs * this.maxTicksPerStep;
            }
            this._lastSampleMs = now;

            // Jitter margin: the render target sits interpTicks behind the
            // newest snapshot, and every 4 ms of jitter adds one more tick of
            // cushion so late packets still arrive before the clock reaches them.
            // Margin moves at most 1 tick per 2 s, so a margin change cannot
            // visibly pop the rendered position.
            let marginTicks = Math.min(6, Math.ceil(this.jitterMs * 4 / this.tickDurationMs));
            if (this._margin < 0) {
                this._margin = marginTicks;
            } else if (marginTicks !== this._margin && now - this._marginChangedMs > 2000) {
                marginTicks = marginTicks > this._margin ? this._margin + 1 : this._margin - 1;
                this._margin = marginTicks;
                this._marginChangedMs = now;
            } else {
                marginTicks = this._margin;
            }
            // Grow/shrink headroom by moving the clock, not the render target:
            // shifting the target alone would pop the interpolated position.
            const desired = this.interpTicks + marginTicks;
            this.renderTick += desired - this._targetTicks;
            this._targetTicks = desired;

            // Wall time → tick time. Alpha is derived from ticks afterwards, so
            // jittered arrivals change how far the clock travels, never where a
            // pair of snapshots sits on the blend.
            const stepTicks = stepMs / this.tickDurationMs;
            this.renderTick += stepTicks;

            const latest = this.snaps[this.snaps.length - 1];
            let target = this.renderTick - this._targetTicks;

            // Buffer too far ahead → accelerate the clock to drain the backlog.
            if (latest.tick - target > this._targetTicks + 3) {
                this.renderTick += stepTicks * (this.speedupFactor - 1.0);
                this._speedupTicks++;
                target = this.renderTick - this._targetTicks;
            }

            // Loss gap: the clock is more than snapTicks behind the newest
            // snapshot with nothing in between → snap once, don't stall.
            if (latest.tick - target > this.snapTicks) {
                this.renderTick = latest.tick + this._targetTicks;
                target = latest.tick;
                // Keep only the snapshots the new render target can still
                // interpolate between, so the base tick stays fresh and the
                // snap does not re-trigger every frame.
                const keepFrom = this.snaps.findIndex(s => s.tick >= latest.tick - this._targetTicks);
                if (keepFrom > 0) this.snaps.splice(0, keepFrom);
                this._snapCount++;
            } else if (target > latest.tick) {
                // Server stalled: freeze on the newest confirmed state instead of
                // running the clock ahead of the data (never extrapolate).
                this.renderTick = latest.tick + this._targetTicks;
                target = latest.tick;
            }

            // Drain the oldest snapshot only once the bracket after it is fully
            // consumed, so the clock can never lose its left-hand side.
            while (this.snaps.length >= 3 && this.snaps[1].tick <= target) this.snaps.shift();

            return this._interpolateAt(target);
        }

        /** The server tick the clock is currently rendering. */
        _targetTick() {
            return this.renderTick - this._targetTicks;
        }

        _interpolateAt(targetTick) {
            const s = this.snaps;
            if (s.length === 0) return null;
            if (targetTick >= s[s.length - 1].tick) return s[s.length - 1].entities;
            if (targetTick <= s[0].tick) return s[0].entities;

            // Last snapshot whose tick is <= targetTick.
            let i = s.length - 1;
            while (i > 0 && s[i].tick > targetTick) i--;
            const a = s[i], b = s[i + 1];
            let alpha = (targetTick - a.tick) / (b.tick - a.tick);
            if (alpha < 0) alpha = 0; else if (alpha > 1) alpha = 1;

            if (this.lerpFn) return this.lerpFn(a.entities, b.entities, alpha);

            const out = {};
            for (const id of Object.keys(b.entities)) {
                const e1 = b.entities[id];
                const e0 = a.entities[id];
                if (!e0) { out[id] = e1; continue; }
                const l = new Float64Array(e1.length);
                for (let k = 0; k < e1.length; k++) {
                    if (this.discrete && this.discrete.includes(k)) { l[k] = e1[k]; continue; }
                    const f = this.fieldLerp && this.fieldLerp[k];
                    l[k] = f ? f(e0[k], e1[k], alpha) : e0[k] + (e1[k] - e0[k]) * alpha;
                }
                out[id] = l;
            }
            return out;
        }

        /** Tuning + health, useful for HUDs and tests. */
        stats() {
            const newest = this.snaps.length ? this.snaps[this.snaps.length - 1].tick : 0;
            return {
                buffered: this.snaps.length,
                tickDurationMs: this.tickDurationMs,
                jitterMs: this.jitterMs,
                renderTick: this._targetTick(),
                speedups: this._speedupTicks,
                snaps: this._snapCount,
                behind: newest - this._targetTick(),
            };
        }

        reset() {
            this.snaps.length = 0;
            this.renderTick = -Infinity;
            this._lastSampleMs = -1;
            this._lastArrivalMs = -1;
            this.jitterMs = 0;
            this._margin = -1;
            this._targetTicks = this.interpTicks;
            this._marginChangedMs = 0;
        }
    }

    /**
     * CYGPredictDriver — the client half of the prediction layer, driven by
     * the guest WASM's predict exports. The SAME module backs the web SDK
     * and the Godot/Unity thin runtimes (they re-implement only the instance
     * plumbing below — tick loop, reconcile pairing and the pending ring are
     * runtime-agnostic logic).
     *
     * Guest contract (feature-detected on the instance's exports; the
     * runtime embed for Unity/Godot is a separate task):
     *   cyg_predict_tick(input_ptr, len, dt_ms)   — advance the sim one step
     *   cyg_predicted_state_ptr() -> i32          — packed predicted-state ptr
     *   cyg_predicted_state_len() -> i32          — its byte length
     *   cyg_reconcile(server_ptr, server_len, pending_ptr, pending_len) -> f64
     *                                             — returns the max correction
     *   cyg_drift_ema() -> f64                    — smoothed correction
     *   cyg_drift_peak() -> f64                   — worst correction seen
     *
     * The web SDK does NOT currently instantiate the guest WASM client-side
     * (only the cyg-core-rs state core) — so `attachPredict(instance)` is
     * the seam: callers (an ECS-template page, a test, a future embed) hand
     * in their own instance and the driver takes over. Without an instance,
     * `room.predict` is a no-op stub with `available === false`.
     */
    class CYGPredictDriver {
        static REQUIRED_EXPORTS = [
            'cyg_predict_tick',
            'cyg_predicted_state_ptr',
            'cyg_predicted_state_len',
            'cyg_reconcile',
            'cyg_drift_ema',
            'cyg_drift_peak',
        ];

        constructor(room, instance = null, opts = {}) {
            this.room = room;
            /** Max unacked inputs kept for reconcile replay (2s at 30Hz). */
            this.maxPending = opts.maxPending ?? 64;
            /** Fixed-step tick rate; 0 = derive from the TimeSync line. */
            this.tickRate = opts.tickRate ?? 0;
            this.instance = null;
            this.available = false;
            /** Unacked inputs, FIFO ring: [{ seq, input: Uint8Array, atMs }] */
            this.pending = [];
            /** Drift mirrors of the guest's cyg_drift_* counters. */
            this.driftEma = 0;
            this.driftPeak = 0;
            /** Fixed-step accumulator state. */
            this._accMs = 0;
            this._lastTickMs = -1;
            this._nextSeq = 1;
            /** Latched edges live OUTSIDE the step loop (see latch()). */
            this._latched = new Map();
            this._lastPredictedState = null;
            /** Optional caller override: (latchedEdges) => number[] input
             *  values, packed f64 in schema field order. */
            this.packInput = opts.packInput || null;
            /** Optional bridge from an authoritative schema delta to the flat
             *  f64 mirror `cyg_reconcile` expects. Without it, callers pass
             *  that flat buffer to `onServerState()` themselves. */
            this.readServerState = opts.readServerState || null;
            if (instance) this.attachPredict(instance, opts);
        }

        /** Feature-detect the guest's predict exports. Missing exports keep
         *  the driver parked — no half-capable guessing. */
        attachPredict(instance, opts = {}) {
            const ex = instance && (instance.exports || instance);
            const missing = ex
                ? CYGPredictDriver.REQUIRED_EXPORTS.filter((n) => typeof ex[n] !== 'function')
                : CYGPredictDriver.REQUIRED_EXPORTS.slice();
            if (missing.length > 0) {
                this.available = false;
                this.instance = null;
                return false;
            }
            this.instance = ex;
            this.available = true;
            this.pending.length = 0;
            if (typeof opts.readServerState === 'function') {
                this.readServerState = opts.readServerState;
            }
            if (opts.packInput) this.packInput = opts.packInput;
            if (opts.tickRate > 0) this.tickRate = opts.tickRate;
            return true;
        }

        /**
         * Send one schema input object through the normal `__input` lane and
         * advance the prediction mirror with the exact same field bytes. The
         * network sequence (`room.inputSeq`) is the prediction sequence, so
         * `room.ackedSeq` trims the same pending ring.
         */
        sendSchema(obj, dtMs = 1000 / 30) {
            if (!this.available || !this.room) return false;
            const pack = this.room._packSchemaInput;
            const send = this.room._sendPackedInput;
            if (typeof pack !== 'function' || typeof send !== 'function') return false;
            const body = pack.call(this.room, obj);
            if (!body || body.byteLength < 2) return false;
            const seq = this.room.inputSeq >>> 0;
            if (!send.call(this.room, body)) return false;
            // Wire body = [seq u16][schema fields]; the guest predict reader
            // starts at the first field, exactly like the server's on_input.
            const frame = body.subarray(2);
            this._predictFrame(frame, dtMs, seq);
            return true;
        }

        _predictFrame(frame, dtMs, seq) {
            const ptr = this._writeBytes(frame);
            this.instance.cyg_predict_tick(ptr, frame.byteLength, dtMs);
            this.pending.push({
                seq,
                input: frame.slice(),
                atMs: performance.now(),
            });
            while (this.pending.length > this.maxPending) this.pending.shift();
        }

        /** Fixed-step accumulator: run `floor(elapsed / step)` steps, capped
         *  so a hitched frame cannot spiral. Returns the send budget n — the
         *  number of inputs consumed (and therefore sent) this frame. */
        tick(now) {
            if (!this.available) return 0;
            const rate = this.tickRate > 0
                ? this.tickRate
                : (this.room && this.room.timeSync ? this.room.timeSync.tickRate : 30);
            const stepMs = 1000 / Math.max(1, rate);
            const elapsed = this._lastTickMs < 0 ? 0 : now - this._lastTickMs;
            this._lastTickMs = now;
            this._accMs += Math.min(250, Math.max(0, elapsed));
            let n = 0;
            while (this._accMs >= stepMs) {
                this._accMs -= stepMs;
                this._step(stepMs);
                n++;
            }
            return n;
        }

        /**
         * Buttons between steps — LATCH the press when it happens, CONSUME
         * it inside the step loop on exactly one step (a tap on a 0-step
         * frame survives to the next step; a press spanning a multi-step
         * frame fires once). `latch(key)` queues an edge; each `_step`
         * drains it exactly once into the packed input.
         */
        latch(key, value = true) { this._latched.set(key, value); }

        _step(dtMs) {
            const packed = this._packInput();
            const seq = this._nextSeq >>> 0;
            this._nextSeq = (this._nextSeq + 1) & 0xFFFF;
            const ptr = this._writeBytes(packed);
            this.instance.cyg_predict_tick(ptr, packed.byteLength, dtMs);
            this.pending.push({ seq, input: packed, atMs: performance.now() });
            while (this.pending.length > this.maxPending) this.pending.shift();
        }

        /** Pack the current input frame — latched edges drained here, ONCE
         *  per step. Default: the room's raw input object flattened to its
         *  number values; rooms can override via `packInput`. */
        _packInput() {
            const src = typeof this.packInput === 'function'
                ? this.packInput(this._latched)
                : [...this._latched.keys()].map((k) => Number(k) || 0);
            // (keys are the latched edge ids — slot numbers in the frame)
            const out = new Float64Array(Math.max(1, src.length));
            for (let i = 0; i < src.length; i++) out[i] = Number(src[i]) || 0;
            this._latched.clear();
            return new Uint8Array(out.buffer);
        }

        /** One server delta acked: replay the unacked pending inputs against
         *  the authoritative state, record the correction, and trim the ring
         *  of everything the server confirmed (ack seq from the delta tail). */
        onAck(serverStateBytes) {
            if (!this.available) return;
            let stateBytes = serverStateBytes;
            if (typeof this.readServerState === 'function') {
                const flat = this.readServerState(serverStateBytes, this.room);
                if (flat && flat.byteLength > 0) {
                    stateBytes = flat instanceof Uint8Array
                        ? flat
                        : new Uint8Array(flat.buffer, flat.byteOffset || 0, flat.byteLength);
                }
            }
            const acked = this.room && Number.isFinite(this.room.ackedSeq)
                ? this.room.ackedSeq : -1;
            const unacked = this.pending.filter(
                (p) => acked < 0 || ((p.seq - acked) & 0xFFFF) < 0x8000 && p.seq !== acked
            );
            const server = this._writeBytes(stateBytes || new Uint8Array(0));
            const flat = this._flattenPending(unacked);
            const pend = this._writeBytes(flat);
            const correction = this.instance.cyg_reconcile(
                server, stateBytes ? stateBytes.byteLength : 0,
                pend, flat.byteLength,
            );
            if (Number.isFinite(correction)) {
                this.driftEma = this.instance.cyg_drift_ema();
                this.driftPeak = this.instance.cyg_drift_peak();
            }
            if (acked >= 0) {
                this.pending = unacked;
            }
        }

        /** Reconcile directly against a flat f64 mirror (used when the
         *  caller's state decoder already produced the authoritative values). */
        onServerState(flatValues) {
            if (!this.available || !flatValues) return;
            const bytes = flatValues instanceof Uint8Array
                ? flatValues
                : new Uint8Array(flatValues.buffer, flatValues.byteOffset || 0, flatValues.byteLength);
            this.onAck(bytes);
        }

        /** Latest predicted state as an object (guest-owned buffer, decoded
         *  through the room's delta decoder when one is attached). */
        predictedState() {
            if (!this.available) return null;
            const ptr = this.instance.cyg_predicted_state_ptr() >>> 0;
            const len = this.instance.cyg_predicted_state_len() >>> 0;
            const bytes = this._readBytes(ptr, len);
            this._lastPredictedState = bytes;
            const decode = this.room && this.room._deltaDecoder;
            if (decode) {
                try { return decode(bytes); } catch (_) { /* opaque buffer */ }
            }
            return bytes;
        }

        /**
         * Render read, Colyseus-style: the predicted value for a LOCAL
         * entity/field, falling through to the interpolation timeline for
         * remotes (the reconciler backs the read while it lives).
         */
        value(entity, field) {
            if (entity != null && this.room && this.room.sessionId != null
                && String(entity) !== String(this.room.playerId)) {
                const tl = this.room.timeline;
                if (tl && typeof tl.sample === 'function') {
                    const snap = tl.sample();
                    return snap && snap.pose ? snap.pose[field] : undefined;
                }
                return undefined;
            }
            const st = this.predictedState();
            return st && typeof st === 'object' ? st[field] : undefined;
        }

        _flattenPending(list) {
            const total = list.reduce((a, p) => a + p.input.byteLength, 0);
            const out = new Uint8Array(total);
            let off = 0;
            for (const p of list) { out.set(p.input, off); off += p.input.byteLength; }
            return out;
        }

        /** Stage bytes into the guest's linear memory and return the pointer.
         *  Uses the optional `cyg_alloc(len)` scratch export when present,
         *  else the tail of the exported `memory` beyond 64 KiB (page-granular
         *  scratch the guest never maps for its own data). Mock instances in
         *  tests provide `cyg_alloc` + `memory` the same way. */
        _writeBytes(bytes) {
            const mem = this.instance.memory;
            if (!mem || !mem.buffer || bytes.byteLength === 0) return 0;
            let ptr = 0;
            if (typeof this.instance.cyg_alloc === 'function') {
                ptr = this.instance.cyg_alloc(bytes.byteLength) >>> 0;
            } else {
                // Scratch base: first byte past 64 KiB — the guest's own
                // data lives below (ABI: globals/heap base occupy page 0).
                ptr = 65536;
            }
            new Uint8Array(mem.buffer, ptr, bytes.byteLength).set(bytes);
            return ptr;
        }

        _readBytes(ptr, len) {
            const mem = this.instance.memory;
            if (!mem || !mem.buffer || len <= 0) return new Uint8Array(0);
            return new Uint8Array(mem.buffer, ptr, len).slice();
        }

        reset() {
            this.pending.length = 0;
            this._accMs = 0;
            this._lastTickMs = -1;
            this._latched.clear();
            this.driftEma = 0;
            this.driftPeak = 0;
        }
    }

    class CYGRollbackPredictor {
        constructor(maxHistory = 64) {
            this.maxHistory = maxHistory;
            this.ringBuffer = new Array(maxHistory).fill(null);
            this.pendingInputs = [];
            this.currentTick = 0;
            this.lastServerTick = 0;
            this.currentState = null;
        }

        /**
         * Execute instant local prediction for the current tick without waiting for server.
         * @param {Object} input - Local player input
         * @param {Function} stepFn - Simulation step function (state, input) => newState
         */
        predict(input, stepFn) {
            this.currentTick++;
            this.pendingInputs.push({ tick: this.currentTick, input });

            if (this.currentState && stepFn) {
                this.currentState = stepFn(this.currentState, input);
            }

            const slot = this.currentTick % this.maxHistory;
            this.ringBuffer[slot] = {
                tick: this.currentTick,
                input: input,
                state: this.currentState ? JSON.parse(JSON.stringify(this.currentState)) : null
            };

            return { tick: this.currentTick, state: this.currentState };
        }

        /**
         * Reconcile local state when authoritative server frame arrives.
         * @param {number} serverTick - The authoritative tick from server
         * @param {Object} authoritativeState - True server state
         * @param {Function} resimulateFn - Resimulate step function (state, input) => newState
         */
        onServerAck(serverTick, authoritativeState, resimulateFn) {
            this.lastServerTick = serverTick;

            // Discard inputs acknowledged by the server
            this.pendingInputs = this.pendingInputs.filter(item => item.tick > serverTick);

            const slot = serverTick % this.maxHistory;
            const historical = this.ringBuffer[slot];

            // If we have local history for this tick, check if prediction was accurate
            let needsRollback = true;
            if (historical && historical.tick === serverTick && historical.state) {
                if (JSON.stringify(historical.state) === JSON.stringify(authoritativeState)) {
                    needsRollback = false;
                }
            }

            if (needsRollback) {
                // Roll back to authoritative server state
                let reconciledState = JSON.parse(JSON.stringify(authoritativeState));

                // Fast-forward: replay all unacknowledged pending inputs up to currentTick
                if (resimulateFn) {
                    for (const pending of this.pendingInputs) {
                        reconciledState = resimulateFn(reconciledState, pending.input);
                        const pendingSlot = pending.tick % this.maxHistory;
                        if (this.ringBuffer[pendingSlot]) {
                            this.ringBuffer[pendingSlot].state = JSON.parse(JSON.stringify(reconciledState));
                        }
                    }
                }

                this.currentState = reconciledState;
                return { rolledBack: true, state: this.currentState, replayedTicks: this.pendingInputs.length };
            }

            return { rolledBack: false, state: this.currentState, replayedTicks: 0 };
        }
    }

    /**
     * Floating schema-state inspector for development.
     *
     * ```js
     * const inspector = new CYG.Inspector(room, { intervalMs: 250 });
     * inspector.attach();
     * ```
     */
    class StateInspector {
        constructor(room, options = {}) {
            this.room = room;
            this.intervalMs = options.intervalMs ?? 250;
            this.el = null;
            this.body = null;
            this.timer = null;
            this._render = () => this.render();
        }

        snapshot() {
            const schema = this.room.schema ? {
                name: this.room.schema.name,
                version: this.room.schema.version,
                hash: '0x' + (this.room.schema.hash >>> 0).toString(16),
                fields: this.room.schema.fields.map(f => ({ name: f.name, type: f.type }))
            } : null;
            let sessionId = null;
            try { sessionId = this.room.sessionId == null ? null : Number(this.room.sessionId); } catch (_) {}
            return {
                schema,
                sessionId,
                playerId: this.room.playerId,
                ready: !!this.room._schemaReady,
                error: this.room._schemaError || null,
                entities: [...this.room.state.values()]
            };
        }

        attach(target = (typeof document !== 'undefined' ? document.body : null)) {
            if (!target || typeof document === 'undefined' || this.el) return this;
            const el = document.createElement('div');
            el.style.cssText = 'position:fixed;right:12px;bottom:12px;width:360px;max-height:46vh;' +
                'display:flex;flex-direction:column;background:rgba(9,9,11,.94);color:#e4e4e7;' +
                'border:1px solid #3f3f46;border-radius:10px;box-shadow:0 12px 40px rgba(0,0,0,.45);' +
                'font:11px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace;z-index:2147483647;overflow:hidden';
            const head = document.createElement('div');
            head.style.cssText = 'display:flex;align-items:center;justify-content:space-between;gap:8px;' +
                'padding:7px 10px;border-bottom:1px solid #27272a;color:#a1a1aa';
            const title = document.createElement('span');
            title.textContent = 'CYG state';
            const close = document.createElement('button');
            close.textContent = '×';
            close.style.cssText = 'all:unset;cursor:pointer;padding:0 4px;color:#a1a1aa;font-size:15px';
            close.onclick = () => this.detach();
            head.append(title, close);
            const body = document.createElement('pre');
            body.style.cssText = 'margin:0;padding:8px 10px;overflow:auto;white-space:pre-wrap;word-break:break-word;color:#d4d4d8';
            el.append(head, body);
            target.appendChild(el);
            this.el = el;
            this.body = body;
            this.render();
            if (!this.timer) this.timer = setInterval(this._render, this.intervalMs);
            return this;
        }

        detach() {
            if (this.timer) { clearInterval(this.timer); this.timer = null; }
            if (this.el && this.el.parentNode) this.el.parentNode.removeChild(this.el);
            this.el = null;
            this.body = null;
            return this;
        }

        render() {
            if (!this.body) return;
            this.body.textContent = JSON.stringify(this.snapshot(), (key, value) => {
                return typeof value === 'bigint' ? Number(value) : value;
            }, 2);
        }
    }

    class CYGClient {
        constructor(endpoint, options = {}) {
            this.endpoint = endpoint;
            this.options = options;
            this._authCallback = null;
            this._authToken = options.token || null;
        }

        /** Register an auth hook. May return a token string or `{ token }`. */
        onAuth(callback) {
            this._authCallback = callback;
            return this;
        }

        /** Backend auth: engine-api ke /v2/account/authenticate/* endpoints.
         *  Token andar store hota hai — future joins automatically use karte hain. */
        auth = {
            /** Device auth: naya/returning device account. Token save hota hai. */
            device: async (deviceId) => this._authenticate('device', { id: deviceId }),
            /** Email + password login (existing account). */
            login: async (email, password) => this._authenticate('email', { email, password }),
            /** Naya email account register karo — POST '' with create:true. */
            register: async (email, password, username) => this._authenticate('', { email, password, username, create: true }),
            /** Apple Game Center credential. */
            gameCenter: async (credential) => this._authenticate('gamecenter', { credential }),
            /** Instant game (Facebook Instant / light platforms). */
            instantGame: async (signedRequest) => this._authenticate('facebookinstantgame', { signed_player_info: signedRequest }),
            /** Custom runtime provider (server-side provider RPC). */
            provider: async (name, data) => this._authenticate('', { provider: name, payload: data ?? {} }),
            /** Token manually set karo (apne storage se restore). */
            setToken: (token) => { this._authToken = token; return this; },
        };

        async _authenticate(kind, body) {
            const base = this._httpBase();
            // kind '' targets the bare provider/email route — no trailing slash,
            // the server's route is exactly `/v2/account/authenticate`.
            const path = kind ? `/${kind}` : '';
            const res = await fetch(`${base}/v2/account/authenticate${path}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body ?? {}),
            });
            if (!res.ok) {
                const text = await res.text().catch(() => '');
                throw new Error(`[CYG SDK] auth ${kind || 'provider'} failed (${res.status}): ${text.slice(0, 120)}`);
            }
            const data = await res.json();
            if (data.token) {
                this._authToken = data.token;
                if (data.user && data.user.sessionId) this._authSessionId = data.user.sessionId;
            }
            return data;
        }

        _httpBase() {
            let e = String(this.endpoint || '').replace(/\/+$/, '');
            if (e.startsWith('ws://')) e = `http://${e.slice(5)}`;
            else if (e.startsWith('wss://')) e = `https://${e.slice(6)}`;
            else if (!/^https?:\/\//i.test(e)) e = `http://${e}`;
            return e;
        }

        _wsBase(seat) {
            const base = new URL(this._httpBase());
            const proto = base.protocol === 'https:' ? 'wss:' : 'ws:';
            const host = base.hostname.startsWith('[') ? base.hostname : base.hostname;
            const gamePort = seat && Number(seat.gamePort) > 0 ? Number(seat.gamePort) : (base.port || (proto === 'wss:' ? '443' : '80'));
            const path = base.pathname && base.pathname !== '/' ? base.pathname.replace(/\/$/, '') : '';
            return `${proto}//${host}:${gamePort}${path}`;
        }

        async _authBody(options) {
            const body = { ...options };
            let token = this._authToken;
            if (typeof this._authCallback === 'function') {
                const value = await this._authCallback();
                if (typeof value === 'string') token = value;
                else if (value && typeof value === 'object' && value.token) token = value.token;
            }
            if (token) body.token = token;
            return { body, token };
        }

        async _reserve(method, roomName, options = {}) {
            if (typeof fetch !== 'function') {
                throw new Error('[CYG SDK] fetch is required for matchmaking');
            }
            const { body, token } = await this._authBody(options);
            const url = `${this._httpBase()}/matchmake/${encodeURIComponent(method)}/${encodeURIComponent(roomName)}`;
            const headers = { 'Content-Type': 'application/json', 'Accept': 'application/json' };
            if (token) headers['Authorization'] = `Bearer ${token}`;
            const response = await fetch(url, {
                method: 'POST',
                headers,
                credentials: 'include',
                body: JSON.stringify(body)
            });
            const text = await response.text();
            if (!response.ok) {
                let detail = text;
                try { detail = JSON.parse(text).error || JSON.parse(text).message || text; } catch (_) {}
                throw this._matchmakeError(`${method}/${roomName} failed: ${detail}`, response.status);
            }
            let seat;
            try { seat = JSON.parse(text); } catch (_) {
                throw this._matchmakeError('matchmake returned invalid JSON', 500);
            }
            if (!seat || !seat.sessionId) {
                throw this._matchmakeError('matchmake reply carried no sessionId', 500);
            }
            this.lastSeat = seat;
            return seat;
        }

        /** Colyseus parity: matchmaking failures throw a MatchMakeError-shaped
         *  error — `err.name === 'MatchMakeError'`, `err.code` is the HTTP
         *  status, so games can branch (404 room unknown, 429 busy, …). */
        _matchmakeError(message, code) {
            const err = new Error(`[CYG SDK] matchmake ${message}`);
            err.name = 'MatchMakeError';
            err.code = code;
            return err;
        }

        async _connectSeat(seat, roomName, options = {}) {
            // The seat's room may be a variant room-key ("duo__region-mumbai")
            // minted by filtered matchmaking — connect to THAT, not the name
            // the caller typed.
            const roomPath = seat.name || seat.room_id || roomName;
            const room = new CYGRoom(this._wsBase(seat), roomPath, {
                ...options,
                sessionId: seat.sessionId,
                reconnectionToken: seat.reconnectionToken || seat.seatToken || null,
                client: this
            });
            await room.connect();
            return room;
        }

        /**
         * Colyseus-style matchmaking. The server mints a seat, then the room
         * opens the WebSocket on the game port with that sessionId.
         */
        async joinOrCreate(roomName, options = {}) {
            const seat = await this._reserve('joinOrCreate', roomName, options);
            return this._connectSeat(seat, roomName, options);
        }

        async join(roomName, options = {}) {
            const seat = await this._reserve('join', roomName, options);
            return this._connectSeat(seat, roomName, options);
        }

        async create(roomName, options = {}) {
            const seat = await this._reserve('create', roomName, options);
            return this._connectSeat(seat, roomName, options);
        }

        async joinById(roomId, options = {}) {
            const seat = await this._reserve('joinById', roomId, options);
            return this._connectSeat(seat, roomId, options);
        }

        async spectate(roomName, options = {}) {
            const seat = await this._reserve('spectate', roomName, options);
            return this._connectSeat(seat, roomName, { ...options, spectator: true });
        }

        async joinSpectator(roomName, options = {}) {
            return this.spectate(roomName, options);
        }

        /**
         * Resume a seat after a reload or a dropped socket — pass the
         * `room.reconnectionToken` from the previous connection. Colyseus
         * parity: a missing token throws MatchMakeError(400). The room's
         * built-in auto-reconnect covers the common case; this is the
         * manual path (fresh page, new process, explicit UX).
         */
        async reconnect(reconnectionToken, roomName = 'game_room', options = {}) {
            if (!reconnectionToken) {
                const err = new Error('[CYG SDK] reconnect() needs a reconnectionToken (from room.reconnectionToken)');
                err.name = 'MatchMakeError';
                err.code = 400;
                throw err;
            }
            const seat = await this._reserve('reconnect', roomName, {
                ...options,
                reconnectionToken
            });
            return this._connectSeat(seat, roomName, options);
        }

        /** List joinable rooms. Optionally filter by room name. */
        async getAvailableRooms(roomName = null) {
            if (typeof fetch !== 'function') {
                throw new Error('[CYG SDK] fetch is required for room discovery');
            }
            const response = await fetch(`${this._httpBase()}/api/rooms`, {
                headers: { 'Accept': 'application/json' },
                credentials: 'include'
            });
            if (!response.ok) {
                throw new Error(`[CYG SDK] room list failed (${response.status})`);
            }
            const payload = await response.json();
            const rooms = Array.isArray(payload) ? payload : (payload.rooms || []);
            return rooms
                .filter(r => !roomName || r.name === roomName || r.roomId === roomName)
                .map(r => ({
                    ...r,
                    roomId: r.roomId || r.name,
                    name: r.name,
                    clients: r.clients ?? r.players ?? 0,
                    maxClients: r.maxClients ?? 0,
                    spectators: r.spectators ?? 0,
                    metadata: r.metadata ?? null
                }));
        }

        static loadReplay(jsonOrBlob) {
            const data = (typeof jsonOrBlob === "string") ? JSON.parse(jsonOrBlob) : jsonOrBlob;
            return new CYGReplayPlayer(data);
        }
    }

    // --- UNIVERSAL HARDWARE KEYCODES (A-Z, 0-9, ARROWS, F1-F12, MOUSE) ---
    const KeyCode = Object.freeze({
        None: 0,

        // Mouse Buttons
        MouseLeft: 1, MouseMiddle: 2, MouseRight: 3, MouseBack: 4, MouseForward: 5,

        // Control & Whitespace
        Backspace: 8, Tab: 9, Enter: 13, Shift: 16, Control: 17, Alt: 18,
        Pause: 19, CapsLock: 20, Escape: 27, Space: 32,

        // Navigation
        PageUp: 33, PageDown: 34, End: 35, Home: 36,
        Insert: 45, Delete: 46, PrintScreen: 44,

        // Arrow Navigation Keys
        ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40,
        Left: 37, Up: 38, Right: 39, Down: 40,

        // Number Keys (0 - 9) Top Row
        Digit0: 48, Digit1: 49, Digit2: 50, Digit3: 51, Digit4: 52,
        Digit5: 53, Digit6: 54, Digit7: 55, Digit8: 56, Digit9: 57,
        Num0: 48, Num1: 49, Num2: 50, Num3: 51, Num4: 52,
        Num5: 53, Num6: 54, Num7: 55, Num8: 56, Num9: 57,

        // Alphabet Keys (A - Z)
        KeyA: 65, KeyB: 66, KeyC: 67, KeyD: 68, KeyE: 69, KeyF: 70,
        KeyG: 71, KeyH: 72, KeyI: 73, KeyJ: 74, KeyK: 75, KeyL: 76,
        KeyM: 77, KeyN: 78, KeyO: 79, KeyP: 80, KeyQ: 81, KeyR: 82,
        KeyS: 83, KeyT: 84, KeyU: 85, KeyV: 86, KeyW: 87, KeyX: 88,
        KeyY: 89, KeyZ: 90,

        // Alphabet Aliases
        A: 65, B: 66, C: 67, D: 68, E: 69, F: 70,
        G: 71, H: 72, I: 73, J: 74, K: 75, L: 76,
        M: 77, N: 78, O: 79, P: 80, Q: 81, R: 82,
        S: 83, T: 84, U: 85, V: 86, W: 87, X: 88,
        Y: 89, Z: 90,

        // System & Meta Keys
        MetaLeft: 91, MetaRight: 92, ContextMenu: 93,

        // Numpad Keys (0 - 9 and Operators)
        Numpad0: 96, Numpad1: 97, Numpad2: 98, Numpad3: 99, Numpad4: 100,
        Numpad5: 101, Numpad6: 102, Numpad7: 103, Numpad8: 104, Numpad9: 105,
        NumpadMultiply: 106, NumpadAdd: 107, NumpadSeparator: 108,
        NumpadSubtract: 109, NumpadDecimal: 110, NumpadDivide: 111,

        // Function Keys (F1 - F12)
        F1: 112, F2: 113, F3: 114, F4: 115, F5: 116, F6: 117,
        F7: 118, F8: 119, F9: 120, F10: 121, F11: 122, F12: 123,

        // Extended Function Keys (F13 - F24)
        F13: 124, F14: 125, F15: 126, F16: 127, F17: 128, F18: 129,
        F19: 130, F20: 131, F21: 132, F22: 133, F23: 134, F24: 135,

        // Locks
        NumLock: 144, ScrollLock: 145,

        // Specific Modifiers
        ShiftLeft: 160, ShiftRight: 161, ControlLeft: 162, ControlRight: 163,
        AltLeft: 164, AltRight: 165,

        // Punctuation & Symbols
        Semicolon: 186, Equal: 187, Comma: 188, Minus: 189,
        Period: 190, Slash: 191, Backquote: 192,
        BracketLeft: 219, Backslash: 220, BracketRight: 221, Quote: 222
    });

    const KeyCodeHelper = Object.freeze({
        isLetter: (key) => key >= 65 && key <= 90,
        isDigit: (key) => (key >= 48 && key <= 57) || (key >= 96 && key <= 105),
        isArrow: (key) => key >= 37 && key <= 40,
        isFunctionKey: (key) => key >= 112 && key <= 123,
        isModifier: (key) => key === 16 || key === 17 || key === 18 || (key >= 160 && key <= 165),
        isMouseButton: (key) => key >= 1 && key <= 5
    });

    const Storage = {
        get(key, fallback = null) {
            try {
                const raw = (typeof localStorage !== 'undefined') ? localStorage.getItem('ox_' + key) : null;
                return raw == null ? fallback : JSON.parse(raw);
            } catch (_) { return fallback; }
        },
        set(key, value) {
            try { localStorage.setItem('ox_' + key, JSON.stringify(value)); } catch (_) {}
            return value;
        },
        remove(key) {
            try { localStorage.removeItem('ox_' + key); } catch (_) {}
        },
    };

    return {
        Client: CYGClient,
        Room: CYGRoom,
        Storage,
        SchemaDef: SchemaDef,
        SchemaMap: SchemaMap,
        Callbacks: Callbacks,
        /** Colyseus v5 naming parity: `getStateCallbacks(room)`. */
        getStateCallbacks: (room) => Callbacks.get(room),
        Inspector: StateInspector,
        Voice: CYGVoiceMesh,
        SignalKind: SignalKind,
        MeshOp: MeshOp,
        MeshEventKind: MeshEventKind,
        AppOp: AppOp,
        MESH_MAX_ENTITIES: MESH_MAX_ENTITIES,
        KeyCode: KeyCode,
        KeyCodeHelper: KeyCodeHelper,
        // Exposed so a native client or a test can check the byte contract directly.
        encodeInputFrame: encodeInputFrame,
        encodeStateFrame: encodeStateFrame,
        encodeEventFrame: encodeEventFrame,
        decodeMeshFrame: decodeMeshFrame,
        isNewerSeq: isNewerSeq,
        // Server star channel (engine_webrtc_protocol::channel parity)
        ChannelOp: ChannelOp,
        AckKind: AckKind,
        DELTA_HEADER_LEN: DELTA_HEADER_LEN,
        ACK_WIRE_LEN: ACK_WIRE_LEN,
        MAX_DELTA_CHUNK: MAX_DELTA_CHUNK,
        MAX_FRAGMENTS: MAX_FRAGMENTS,
        FRAGMENT_TTL_MS: FRAGMENT_TTL_MS,
        DELTA_TAIL_LEN: DELTA_TAIL_LEN,
        parseDeltaFrame: parseDeltaFrame,
        encodeDeltaFrame: encodeDeltaFrame,
        encodeAck: encodeAck,
        DeltaSequencer: DeltaSequencer,
        ReplayPlayer: CYGReplayPlayer,
        RollbackPredictor: CYGRollbackPredictor,
        InterpolationBuffer: CYGInterpolationBuffer,
        InterpolationTimeline: CYGInterpolationTimeline,
        TimeSync: CYGTimeSync,
        PredictDriver: CYGPredictDriver,
        INPUT_STAMP_MAGIC: INPUT_STAMP_MAGIC,
        RTT_EMA_ALPHA: RTT_EMA_ALPHA,
        version: "1.5.0"
    };
};
export default factory();
