// The browser half of game/src/socket.rs: a WebSocket the Rust side can poll.
// Loaded after gl.js (miniquad's glue) and before load("game.wasm").
//
// Rust and JavaScript only share numbers and the wasm memory, so strings and
// byte buffers are passed as (pointer, length) into that memory.
(function () {
    var ws = null;
    var inbox = []; // received messages (Uint8Array), oldest first

    function mem(ptr, len) {
        return new Uint8Array(wasm_memory.buffer, ptr, len);
    }

    function net_close() {
        if (ws) {
            ws.onmessage = null;
            ws.close();
            ws = null;
        }
    }

    function net_connect(ptr, len) {
        net_close();
        inbox = [];
        try {
            ws = new WebSocket(new TextDecoder().decode(mem(ptr, len)));
            ws.binaryType = "arraybuffer";
            ws.onmessage = function (e) {
                if (e.data instanceof ArrayBuffer) inbox.push(new Uint8Array(e.data));
            };
        } catch (err) {
            console.error("WebSocket connect failed:", err);
            ws = null; // reads as "closed"
        }
    }

    // 0 connecting, 1 open, 2 closed (also: never opened, or failed)
    function net_state() {
        if (!ws) return 2;
        return ws.readyState === 0 ? 0 : ws.readyState === 1 ? 1 : 2;
    }

    function net_send(ptr, len) {
        // slice() copies: the wasm memory may move or change before the browser sends.
        if (ws && ws.readyState === 1) ws.send(mem(ptr, len).slice());
    }

    // Returns the message length, -1 if nothing is waiting, -2 if it does not fit.
    function net_recv(ptr, cap) {
        if (inbox.length === 0) return -1;
        var m = inbox.shift();
        if (m.length > cap) return -2;
        mem(ptr, cap).set(m);
        return m.length;
    }

    miniquad_add_plugin({
        register_plugin: function (importObject) {
            importObject.env.net_connect = net_connect;
            importObject.env.net_state = net_state;
            importObject.env.net_send = net_send;
            importObject.env.net_recv = net_recv;
            importObject.env.net_close = net_close;
        },
        on_init: function () {},
    });
})();
