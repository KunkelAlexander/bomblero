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

// Draws a texture as it is. Used where the blend mode (not the shader) does the
// work, e.g. multiplying the lightmap over the scene.
pub const COPY_FRAGMENT: &str = "#version 100
precision mediump float;
varying vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = texture2D(Texture, uv);
}
";

// One light, drawn as a square of the lightmap around it. `world` is the
// position of each pixel in game coordinates (handed over by the vertex shader).
pub const LIGHT_VERTEX: &str = "#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying vec2 world;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    world = position.xy;
}
";

// Brightness falls off smoothly with distance. Shadows: walk from the pixel to
// the light and add up how much wall we pass through (the mask has one texel per
// tile, 1 where light cannot pass). Behind a wall that is plenty, so it is dark;
// the first few pixels *inside* a wall facing the light get a fading edge of
// light, which is what makes walls look lit from the side.
// The loop has a fixed count because GLSL 100 (WebGL) demands it; the step length
// grows with distance, so a 32 px wall is never stepped over.
pub const LIGHT_FRAGMENT: &str = "#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
varying vec2 world;
uniform vec2 light;
uniform float radius;
uniform vec3 tint; // light colour times strength
uniform vec2 map_px; // size of the map in pixels
uniform sampler2D Mask;

void main() {
    vec2 to_light = light - world;
    float d = length(to_light);
    if (d >= radius) {
        discard;
    }
    float in_wall = 0.0;
    float step_px = d / 24.0;
    for (int i = 0; i < 24; i++) {
        vec2 p = world + to_light * (float(i) / 24.0);
        in_wall += texture2D(Mask, p / map_px).r * step_px;
    }
    float lit = 1.0 - clamp(in_wall / 14.0, 0.0, 1.0);
    float fall = 1.0 - d / radius;
    gl_FragColor = vec4(tint * (fall * fall * lit), 1.0);
}
";

// The gloss of the black walls: a mirror-like highlight (a "specular" term, the
// standard shiny-surface formula) from each light, drawn at full resolution on
// top of the lit scene. A highlight is bright only where the surface leans so
// that it reflects the light towards the eye, so it shows up as a thin glint
// along the edge of a wall that faces the light, and it slides along the edge as
// the light moves. `Normals` is baked in terrain.rs: red/green hold the
// direction the surface leans (0.5 = flat), alpha says whether there is wall.
// The pass runs for walls only, so everywhere else it exits on the first line.
pub const GLOSS_FRAGMENT: &str = "#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
varying vec2 world;
uniform vec2 light;
uniform float radius;
uniform vec3 tint;
uniform vec2 map_px;
uniform sampler2D Mask;
uniform sampler2D Normals;

void main() {
    vec2 uv = world / map_px;
    if (uv.x < 0.0 || uv.y < 0.0 || uv.x > 1.0 || uv.y > 1.0) {
        discard;
    }
    vec4 surface = texture2D(Normals, uv);
    if (surface.a < 0.02) {
        discard;
    }
    vec2 to_light = light - world;
    float d = length(to_light);
    if (d >= radius) {
        discard;
    }
    // Walls hide the light from each other, exactly as in the lightmap.
    float in_wall = 0.0;
    float step_px = d / 16.0;
    for (int i = 0; i < 16; i++) {
        vec2 p = world + to_light * (float(i) / 16.0);
        in_wall += texture2D(Mask, p / map_px).r * step_px;
    }
    float lit = 1.0 - clamp(in_wall / 14.0, 0.0, 1.0);

    // The surface direction, tilted more the further it is from flat. The light
    // hangs 40 px above the floor and the eye looks straight down, so the
    // highlight is where the surface faces halfway between the two.
    vec3 n = normalize(vec3((surface.rg - 0.5) * 2.8, 1.0));
    vec3 l = normalize(vec3(to_light, 40.0));
    vec3 h = normalize(l + vec3(0.0, 0.0, 1.0));
    float spec = pow(max(dot(n, h), 0.0), 40.0);
    float fall = 1.0 - d / radius;
    gl_FragColor = vec4(tint * (spec * lit * fall * 2.4 * surface.a), 1.0);
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
uniform float hud_edge; // uv.y below this is the HUD strip: no glow there

// Bright-pass: only the part of a pixel above a threshold may glow. The grey
// arena sits below it and stays crisp; players, bolts and explosions are
// above it and bleed light. Neon works through selective brightness: if every
// tile glows, nothing stands out.
vec3 bright(vec2 p) {
    return max(texture2D(Texture, p).rgb - 0.55, 0.0) * 2.2;
}

void main() {
    vec3 sharp = texture2D(Texture, uv).rgb;
    vec3 glow = vec3(0.0);
    // Two rings of 8 taps each: a near ring and a far ring.
    for (int i = 0; i < 8; i++) {
        float a = float(i) * 0.785398; // 45 degrees
        vec2 dir = vec2(cos(a), sin(a));
        glow += bright(uv + dir * texel * 4.0);
        glow += bright(uv + dir * texel * 10.0) * 0.6;
    }
    glow /= 12.8;
    // Glow makes small text fuzzy, so the HUD strip is drawn sharp.
    float in_world = step(hud_edge, uv.y);
    gl_FragColor = vec4(sharp + glow * 1.6 * in_world, 1.0);
}
";
