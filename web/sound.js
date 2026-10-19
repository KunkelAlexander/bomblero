// Keeps the game's audio alive in the browser. Loaded after audio.js (quad-snd's
// glue, copied from the quad-snd crate), which creates the `audio_context`.
//
// Browsers start an AudioContext "suspended" and only let a tap, click or key
// press wake it (the autoplay policy). audio.js tries once, on the *first*
// touchstart, and then gives up; iPhones only accept a touchend, so there the
// sound never starts. This keeps trying on every gesture until it is running,
// and wakes it again if the system suspends it (a call, a locked screen).
(function () {
    function wake() {
        if (typeof audio_context !== "undefined" && audio_context && audio_context.state !== "running") {
            audio_context.resume();
        }
    }
    ["touchend", "mousedown", "keydown"].forEach(function (e) {
        document.addEventListener(e, wake);
    });
    // A hidden tab should be silent; coming back wakes it.
    document.addEventListener("visibilitychange", function () {
        if (typeof audio_context === "undefined" || !audio_context) return;
        if (document.hidden) audio_context.suspend();
        else audio_context.resume();
    });
})();
