mergeInto(LibraryManager.library, {
    OpenOxide_WS_Connect: function (urlPtr, onOpenCb, onMessageCb, onErrorCb, onCloseCb) {
        var url = UTF8ToString(urlPtr);
        try {
            var ws = new WebSocket(url);
            ws.binaryType = 'arraybuffer';

            if (!window._openoxide_sockets) window._openoxide_sockets = {};
            if (!window._openoxide_next_id) window._openoxide_next_id = 1;
            var socketId = window._openoxide_next_id++;
            window._openoxide_sockets[socketId] = ws;

            ws.onopen = function () {
                {{{ makeDynCall('vi', 'onOpenCb') }}}(socketId);
            };

            ws.onmessage = function (ev) {
                if (ev.data instanceof ArrayBuffer) {
                    var bytes = new Uint8Array(ev.data);
                    var len = bytes.length;
                    var ptr = _malloc(len);
                    HEAPU8.set(bytes, ptr);
                    {{{ makeDynCall('viii', 'onMessageCb') }}}(socketId, ptr, len);
                    _free(ptr);
                }
            };

            ws.onerror = function () {
                {{{ makeDynCall('vi', 'onErrorCb') }}}(socketId);
            };

            ws.onclose = function () {
                {{{ makeDynCall('vi', 'onCloseCb') }}}(socketId);
                delete window._openoxide_sockets[socketId];
            };

            return socketId;
        } catch (e) {
            console.error('[OpenOxide JS] WebSocket connection error:', e);
            return -1;
        }
    },

    OpenOxide_WS_Send: function (socketId, ptr, length) {
        var ws = window._openoxide_sockets && window._openoxide_sockets[socketId];
        if (ws && ws.readyState === 1) {
            var data = HEAPU8.subarray(ptr, ptr + length);
            ws.send(data);
        }
    },

    OpenOxide_WS_Close: function (socketId) {
        var ws = window._openoxide_sockets && window._openoxide_sockets[socketId];
        if (ws) {
            ws.close();
            delete window._openoxide_sockets[socketId];
        }
    }
});
