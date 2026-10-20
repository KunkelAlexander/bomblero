// What kind of device the game runs on, for game/src/touch.rs.
//
// "pointer: coarse" means the main pointer is a finger: phones and tablets.
// A laptop with a touchscreen reports "fine" (its main pointer is the mouse),
// so it starts in mouse mode and only switches to touch when touched.
(function () {
    var coarse = window.matchMedia && window.matchMedia("(pointer: coarse)").matches;

    // On a phone, the first tap asks for fullscreen (no address bar eating the
    // short side of a landscape screen) and then locks landscape. Browsers only
    // allow this from a user gesture, hence the tap. iPhone Safari supports
    // neither for web pages; there the game's "turn your device" hint remains.
    function landscape() {
        var el = document.documentElement;
        if (document.fullscreenElement || !el.requestFullscreen) return;
        el.requestFullscreen().then(function () {
            if (screen.orientation && screen.orientation.lock) return screen.orientation.lock("landscape");
        }).catch(function () {});
    }
    if (coarse) document.addEventListener("touchend", landscape, { once: true });

    // The browser drops fullscreen on its own now and then (app switch, back
    // gesture, notification shade) and the one-shot tap above is gone by then.
    // So a small button, shown only while we are NOT fullscreen, asks again.
    // It is plain DOM rather than game UI: this needs no wasm rebuild and a
    // click is the user gesture the browser demands.
    if (coarse && document.documentElement.requestFullscreen) {
        var btn = document.createElement("button");
        btn.textContent = "⛶";
        btn.setAttribute("aria-label", "Fullscreen");
        btn.style.cssText = "position:fixed;top:8px;left:50%;transform:translateX(-50%);" +
            "width:44px;height:44px;font-size:24px;line-height:1;padding:0;border-radius:8px;" +
            "border:1px solid #0ff;background:rgba(0,0,0,.5);color:#0ff;opacity:.6;" +
            "z-index:10;touch-action:manipulation;-webkit-tap-highlight-color:transparent;";
        btn.addEventListener("click", landscape);
        document.body.appendChild(btn);
        var sync = function () { btn.style.display = document.fullscreenElement ? "none" : "block"; };
        document.addEventListener("fullscreenchange", sync);
        sync();
    }

    miniquad_add_plugin({
        register_plugin: function (importObject) {
            importObject.env.device_is_touch = function () { return coarse ? 1 : 0; };
        },
        on_init: function () {},
    });
})();
