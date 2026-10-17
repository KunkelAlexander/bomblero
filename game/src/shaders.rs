// Shaders live in the source as strings (no asset files).
//
// Why GLSL 100: it is the one dialect that both WebGL (browser) and desktop
// OpenGL accept through macroquad, so we write each shader once.

pub const VERTEX: &str = "#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    color = color0 / 255.0;
    uv = texcoord;
}
";

// Cheap glow: average a ring of neighbouring pixels (a blur) and add it on top
// of the sharp image. Bright thin lines bleed light into the dark around them,
// which is the whole "neon" look. M3 replaces this with a proper two-pass
// blur at lower resolution; for M0 it proves that custom shaders and render
// targets work in the browser.
pub const GLOW_FRAGMENT: &str = "#version 100
precision mediump float;
varying vec2 uv;
varying vec4 color;
uniform sampler2D Texture;
uniform vec2 texel; // size of one pixel in uv units

void main() {
    vec3 sharp = texture2D(Texture, uv).rgb;
    vec3 glow = vec3(0.0);
    // Two rings of 8 taps each: a near ring and a far ring.
    for (int i = 0; i < 8; i++) {
        float a = float(i) * 0.785398; // 45 degrees
        vec2 dir = vec2(cos(a), sin(a));
        glow += texture2D(Texture, uv + dir * texel * 4.0).rgb;
        glow += texture2D(Texture, uv + dir * texel * 10.0).rgb * 0.6;
    }
    glow /= 12.8;
    gl_FragColor = vec4(sharp + glow * 1.6, 1.0);
}
";
