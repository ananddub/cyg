/**
 * # OpenOxide Gaming SDK (Client)
 *
 * Lightweight, zero-dependency, ultra-fast multiplayer game client SDK.
 * Features:
 * - Sub-millisecond binary delta streaming
 * - Authoritative tick alignment
 * - Native Spectator Mode (read-only observer)
 * - Match Replay Recording & Frame Playback (.oxr format)
 * - WebRTC voice mesh: peer-to-peer Opus with a server-relayed PCM fallback
 */
(function (root, factory) {
    if (typeof define === 'function' && define.amd) {
        define([], factory);
    } else if (typeof module === 'object' && module.exports) {
        module.exports = factory();
    } else {
        root.OpenOxide = factory();
    }
}(typeof self !== 'undefined' ? self : this, function () {

    // --- WEBRTC SIGNALING WIRE CODEC (mirrors engine_voice::signaling) ---
    // ['W'][version u8][kind u8][from u32][to u32][opaque JSON body], big-endian.
    const VOICE_MAGIC = 0x56;
    const SIGNAL_MAGIC = 0x57;
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

    class OpenOxideRoom {
        constructor(endpoint, roomName, options = {}) {
            this.endpoint = endpoint.replace(/^http/, 'ws');
            this.roomName = roomName;
            this.options = options;
            this.isSpectator = !!options.spectator;
            this.ws = null;
            this.sessionId = null;
            this.isOpen = false;
            this.listeners = new Map();
            // Attached WebRTC voice mesh, or null when voice is relay-only.
            this.voice = null;

            // Replay recording state
            this.recording = false;
            this.recordStartTime = 0;
            this.recordedFrames = [];

            // QUIC-style protocol sequence counters
            this.inputSeq = 0;
            // WebRTC Client-Server DataChannels (str0m UDP star topology)
            this.serverPc = null;
            this.serverChUnreliable = null;
            this.serverChReliable = null;
            this.serverRtcActive = false;
            this.serverRtcConnecting = false;
        }

        connect() {
            return new Promise((resolve, reject) => {
                let cleanUrl = this.endpoint.endsWith('/') ? this.endpoint : `${this.endpoint}/`;
                let wsUrl = `${cleanUrl}${this.roomName}`;
                if (this.isSpectator) {
                    wsUrl += '?spectator=1';
                }

                try {
                    this.ws = new WebSocket(wsUrl);
                    this.ws.binaryType = 'arraybuffer';
                } catch (e) {
                    return reject(e);
                }

                this.ws.onopen = () => {
                    this.isOpen = true;
                    if (this.isSpectator) {
                        this.ws.send(new Uint8Array([0xFF, 0x53, 0x50, 0x45, 0x43]));
                    }
                    this.emit('open');
                    this.connectServerWebRtc();
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
                    this.emit('close', ev);
                };

                this.ws.onmessage = (ev) => {
                    let bytes = null;
                    if (ev.data instanceof ArrayBuffer) {
                        bytes = new Uint8Array(ev.data);
                    } else if (ev.data && ev.data.buffer) {
                        bytes = new Uint8Array(ev.data.buffer, ev.data.byteOffset || 0, ev.data.byteLength || ev.data.length);
                    } else if (typeof Buffer !== 'undefined' && Buffer.isBuffer && Buffer.isBuffer(ev.data)) {
                        bytes = new Uint8Array(ev.data);
                    }

                    if (bytes) {
                        this._handleInboundBinary(bytes);
                    }
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

                chUnreliable.onopen = () => {
                    console.log('[OpenOxide SDK] ⚡ WebRTC Server "game-unreliable" channel OPEN (UDP)!');
                    this.serverRtcActive = true;
                    this.serverRtcConnecting = false;
                    this.emit('server-webrtc-open');
                };

                chReliable.onopen = () => {
                    console.log('[OpenOxide SDK] ⚡ WebRTC Server "game-reliable" channel OPEN (SCTP)!');
                    this.emit('server-webrtc-reliable-open');
                };

                chUnreliable.onclose = () => {
                    console.warn('[OpenOxide SDK] WebRTC Server "game-unreliable" channel closed');
                    this.serverRtcActive = false;
                };

                chReliable.onclose = () => {
                    console.warn('[OpenOxide SDK] WebRTC Server "game-reliable" channel closed');
                };

                chUnreliable.onerror = (e) => {
                    console.warn('[OpenOxide SDK] WebRTC Server unreliable channel error:', e);
                };

                chReliable.onerror = (e) => {
                    console.warn('[OpenOxide SDK] WebRTC Server reliable channel error:', e);
                };

                chUnreliable.onmessage = (ev) => {
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

                chReliable.onmessage = (ev) => {
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
                    console.log(`[OpenOxide SDK] ⚡ Server WebRTC connection state: ${pc.connectionState}`);
                    if (pc.connectionState === 'connected') {
                        this.serverRtcActive = true;
                        this.serverRtcConnecting = false;
                        this.emit('server-webrtc-open');
                    } else if (pc.connectionState === 'disconnected' || pc.connectionState === 'failed') {
                        this.serverRtcActive = false;
                    }
                };

                pc.createOffer().then(offer => {
                    return pc.setLocalDescription(offer);
                }).then(() => {
                    this.sendSignal(SignalKind.OFFER, 0, { sdp: pc.localDescription.sdp });
                    console.log('[OpenOxide SDK] ⚡ WebRTC SDP Offer sent to Server');
                }).catch(err => {
                    console.warn('[OpenOxide SDK] WebRTC server createOffer error:', err);
                    this.serverRtcConnecting = false;
                });
            } catch (err) {
                console.warn('[OpenOxide SDK] WebRTC server setup error:', err);
                this.serverRtcConnecting = false;
            }
        }

        _handleInboundBinary(bytes) {
            if (!bytes || bytes.length === 0) return;

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
                const ev = decodeEventFrame(evBytes);
                if (ev) {
                    this.emit('event', ev);
                    return;
                }
            }

            let offset = 0;
            if (bytes.length >= 15 && bytes[0] === 0x11 && bytes[1] === 0xA5) {
                offset = 7;
            } else if (bytes.length >= 14 && bytes[0] === 0xA5 && bytes[1] === 0x64) {
                offset = 6;
            }

            if (offset > 0) {
                const payload = bytes.subarray(offset);

                if (this.sessionId == null && payload.length >= 8) {
                    const v = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
                    this.sessionId = v.getBigInt64(0, true);
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

                this.emit('delta', payload);
            }
        }

        /**
         * Stream a 15-byte header + Opus voice packet to the server
         */
        sendVoice(channelMode, seq, timestamp, flags, opusPayload) {
            if (this.isSpectator || !this.ws || this.ws.readyState !== WebSocket.OPEN) return;
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
            this.ws.send(buf);
        }

        /**
         * Set current squad/team ID on server voice router
         */
        setVoiceTeam(teamId) {
            if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
            this.ws.send(new Uint8Array([VOICE_MAGIC, 0x54, teamId])); // 'V', 'T', teamId
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
                            console.log('[OpenOxide SDK] ⚡ WebRTC Server SDP Answer accepted!');
                        }).catch(e => {
                            console.warn('[OpenOxide SDK] setRemoteDescription failed:', e);
                        });
                    }
                    return;
                } else if (frame.kind === SignalKind.ICE && this.serverPc) {
                    if (frame.body && frame.body.candidate) {
                        this.serverPc.addIceCandidate(new RTCIceCandidate({
                            candidate: frame.body.candidate,
                            sdpMid: frame.body.sdpMid,
                            sdpMLineIndex: frame.body.sdpMLineIndex
                        })).catch(e => console.warn('[OpenOxide SDK] addIceCandidate failed:', e));
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
            this.voice = new OpenOxideVoiceMesh(this, options);
            return this.voice;
        }

        /**
         * Stream a 23-byte QUIC-style unordered input frame to the authoritative server
         */
        sendInput(flags, aimYaw, aimPitch, moveX, moveZ) {
            if (this.isSpectator) {
                return;
            }

            this.inputSeq = (this.inputSeq + 1) & 0xFFFF;
            const buf = encodeInputFrame({
                seq: this.inputSeq,
                flags,
                aimYaw,
                aimPitch,
                moveX,
                moveZ
            });

            // Pure WebRTC DataChannel (UDP, 0 retransmits, no TCP HOL blocking):
            // When WebRTC is active, NO message is sent on WebSocket!
            if (this.serverChUnreliable && this.serverChUnreliable.readyState === 'open') {
                this.serverRtcActive = true;
                try {
                    this.serverChUnreliable.send(buf);
                    // Guarantee fire pulse delivery over SCTP reliable channel
                    if ((flags & 16) && this.serverChReliable && this.serverChReliable.readyState === 'open') {
                        this.sendEvent(1, 0xFFFF, 0, 1.0);
                    }
                    return;
                } catch (e) {
                    console.warn('[OpenOxide SDK] serverChUnreliable.send error:', e);
                }
            }

            // Fallback to WebSocket only while WebRTC connection is negotiating
            if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                this.ws.send(buf);
            }
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
                    console.warn('[OpenOxide SDK] serverChReliable.send error:', e);
                }
            }

            // Fallback to WebSocket only while WebRTC connection is negotiating
            if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                this.ws.send(buf);
            }
        }

        leave() {
            this.serverRtcActive = false;
            if (this.serverPc) {
                try { this.serverPc.close(); } catch (_) {}
                this.serverPc = null;
            }
            if (this.ws) {
                try { this.ws.close(); } catch (_) {}
            }
            if (this.voice) {
                this.voice.dispose();
            }
        }

        /**
         * Generic byte send
         */
        sendBytes(type, bytes) {
            if (this.ws && this.ws.readyState === WebSocket.OPEN) {
                this.ws.send(bytes);
            }
        }

        // --- REPLAY RECORDER (reference Replay Equivalent) ---

        startRecording() {
            this.recording = true;
            this.recordStartTime = performance.now();
            this.recordedFrames = [];
            console.log(`[OpenOxide SDK] ⏺️ Match recording started for room "${this.roomName}"`);
        }

        stopRecording() {
            this.recording = false;
            const duration = Math.round(performance.now() - this.recordStartTime);
            console.log(`[OpenOxide SDK] ⏹️ Recording captured ${this.recordedFrames.length} frames (${duration}ms)`);
            return {
                format: "oxr-v1",
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

        emit(event, ...args) {
            const list = this.listeners.get(event);
            if (list) {
                for (let i = 0; i < list.length; i++) {
                    list[i](...args);
                }
            }
        }

        leave() {
            if (this.ws) {
                this.ws.close();
            }
        }
    }

    /**
     * WebRTC voice mesh for one room.
     *
     * Every remote peer gets its own `RTCPeerConnection` carrying the local mic
     * track, so Opus media flows browser-to-browser and never touches the game
     * server. The server only relays negotiation frames (see
     * `OpenOxideRoom.sendSignal`), so it stays authoritative about who is in the
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
    class OpenOxideVoiceMesh {
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
                    ordered: false, maxRetransmits: 0, protocol: 'ox-mesh-1'
                });
                entry.chReliable = pc.createDataChannel('game-reliable', {
                    ordered: true, protocol: 'ox-mesh-1'
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
    class OpenOxideReplayPlayer {
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
     * GGPO / reference reference-Style Client Predict & Rollback Engine.
     * Manages client-side inputs, instant zero-latency local prediction,
     * historical frame snapshots, and authoritative state reconciliations.
     */
    class OpenOxideRollbackPredictor {
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

    class OpenOxideClient {
        constructor(endpoint) {
            this.endpoint = endpoint;
        }

        async joinOrCreate(roomName, options = {}) {
            const room = new OpenOxideRoom(this.endpoint, roomName, { ...options, spectator: false });
            await room.connect();
            return room;
        }

        async spectate(roomName, options = {}) {
            const room = new OpenOxideRoom(this.endpoint, roomName, { ...options, spectator: true });
            await room.connect();
            return room;
        }

        async joinSpectator(roomName, options = {}) {
            return this.spectate(roomName, options);
        }

        static loadReplay(jsonOrBlob) {
            const data = (typeof jsonOrBlob === "string") ? JSON.parse(jsonOrBlob) : jsonOrBlob;
            return new OpenOxideReplayPlayer(data);
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

    return {
        Client: OpenOxideClient,
        Room: OpenOxideRoom,
        Voice: OpenOxideVoiceMesh,
        SignalKind: SignalKind,
        MeshOp: MeshOp,
        MeshEventKind: MeshEventKind,
        MESH_MAX_ENTITIES: MESH_MAX_ENTITIES,
        KeyCode: KeyCode,
        KeyCodeHelper: KeyCodeHelper,
        // Exposed so a native client or a test can check the byte contract directly.
        encodeInputFrame: encodeInputFrame,
        encodeStateFrame: encodeStateFrame,
        encodeEventFrame: encodeEventFrame,
        decodeMeshFrame: decodeMeshFrame,
        isNewerSeq: isNewerSeq,
        ReplayPlayer: OpenOxideReplayPlayer,
        RollbackPredictor: OpenOxideRollbackPredictor,
        version: "1.3.0"
    };
}));

