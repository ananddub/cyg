mergeInto(LibraryManager.library, {
    OpenOxide_WS_Connect: function (urlPtr) {
        var url = UTF8ToString(urlPtr);
        try {
            var ws = new WebSocket(url);
            ws.binaryType = 'arraybuffer';

            if (!window._openoxide_sockets) window._openoxide_sockets = {};
            if (!window._openoxide_next_id) window._openoxide_next_id = 1;
            var socketId = window._openoxide_next_id++;

            var sockObj = {
                ws: ws,
                queue: [],
                state: 0 // 0 = connecting, 1 = open, 2 = closing, 3 = closed, 4 = error
            };

            ws.onopen = function () {
                sockObj.state = 1;
            };

            ws.onmessage = function (ev) {
                if (ev.data instanceof ArrayBuffer) {
                    sockObj.queue.push(new Uint8Array(ev.data));
                }
            };

            ws.onerror = function () {
                sockObj.state = 4;
            };

            ws.onclose = function () {
                sockObj.state = 3;
            };

            window._openoxide_sockets[socketId] = sockObj;
            return socketId;
        } catch (e) {
            console.error('[OpenOxide JS] WebSocket connection error:', e);
            return -1;
        }
    },

    OpenOxide_WS_GetState: function (socketId) {
        var s = window._openoxide_sockets && window._openoxide_sockets[socketId];
        if (!s) return 3; // closed
        return s.state;
    },

    OpenOxide_WS_PollPacketSize: function (socketId) {
        var s = window._openoxide_sockets && window._openoxide_sockets[socketId];
        if (!s || s.queue.length === 0) return 0;
        return s.queue[0].length;
    },

    OpenOxide_WS_PollPacket: function (socketId, dstPtr, maxLen) {
        var s = window._openoxide_sockets && window._openoxide_sockets[socketId];
        if (!s || s.queue.length === 0) return 0;
        var packet = s.queue.shift();
        var copyLen = Math.min(packet.length, maxLen);
        HEAPU8.set(packet.subarray(0, copyLen), dstPtr);
        return copyLen;
    },

    OpenOxide_WS_Send: function (socketId, ptr, length) {
        var s = window._openoxide_sockets && window._openoxide_sockets[socketId];
        if (s && s.ws && s.ws.readyState === 1) {
            var data = HEAPU8.slice(ptr, ptr + length);
            s.ws.send(data);
        }
    },

    OpenOxide_WS_Close: function (socketId) {
        var s = window._openoxide_sockets && window._openoxide_sockets[socketId];
        if (s) {
            try { s.ws.close(); } catch (e) {}
            s.state = 3;
            s.queue = [];
            delete window._openoxide_sockets[socketId];
        }
    }
});
