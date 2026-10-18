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

    miniquad_add_plugin({
        register_plugin: function (importObject) {
            importObject.env.device_is_touch = function () { return coarse ? 1 : 0; };
        },
        on_init: function () {},
    });
})();
