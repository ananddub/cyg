/**
 * # OpenOxide Gaming SDK (Client)
 *
 * Lightweight, zero-dependency, ultra-fast multiplayer game client SDK.
 * Features:
 * - Sub-millisecond binary delta streaming
 * - Authoritative tick alignment
 * - Native Spectator Mode (read-only observer)
 * - Match Replay Recording & Frame Playback (.oxr format)
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

            // Replay recording state
            this.recording = false;
            this.recordStartTime = 0;
            this.recordedFrames = [];
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
                    resolve(this);
                };

                this.ws.onerror = (err) => {
                    this.emit('error', err);
                    if (!this.isOpen) reject(err);
                };

                this.ws.onclose = (ev) => {
                    this.isOpen = false;
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
                        let offset = 0;
                        if (bytes.length >= 15 && bytes[0] === 0x11 && bytes[1] === 0xA5) {
                            offset = 7;
                        } else if (bytes.length >= 14 && bytes[0] === 0xA5 && bytes[1] === 0x64) {
                            offset = 6;
                        }

                        if (offset > 0) {
                            const payload = bytes.subarray(offset);

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
                };
            });
        }

        /**
         * Stream a 20-byte packed player input frame to the authoritative server
         */
        sendInput(flags, aimYaw, aimPitch, moveX, moveZ) {
            if (this.isSpectator || !this.ws || this.ws.readyState !== WebSocket.OPEN) {
                return;
            }

            const buf = new ArrayBuffer(20);
            const view = new DataView(buf);
            view.setUint32(0, flags, true);
            view.setFloat32(4, aimYaw, true);
            view.setFloat32(8, aimPitch, true);
            view.setFloat32(12, moveX, true);
            view.setFloat32(16, moveZ, true);

            this.ws.send(buf);
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

    return {
        Client: OpenOxideClient,
        Room: OpenOxideRoom,
        ReplayPlayer: OpenOxideReplayPlayer,
        version: "1.0.0"
    };
}));
