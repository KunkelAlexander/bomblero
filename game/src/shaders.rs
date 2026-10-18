// Shaders live in the source as strings (no asset files).
//
// Why GLSL 100: it is the one dialect that both WebGL (browser) and desktop
// OpenGL accept through macroquad, so we write each shader once.

pub const VERTEX: &str = "#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
// mediump: lowp (8 bits on phones) cannot address single pixels of a
// screen-sized texture, which the glow's taps rely on.
varying mediump vec2 uv;
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

// The glow ("bloom"), in three small steps. Real neon has a white-hot tube, a
// tight bright glow around it and a faint halo that lights the air. A
// blur of the bright parts gives exactly that, and because it is done on
// smaller copies of the picture it is cheap: a half-size copy has a quarter
// of the pixels, and each halving doubles how far one blur step reaches.
//
//   1. DOWN:    shrink the picture to half size, keeping only what is bright.
//               Shrink that again for the wider level.
//   2. BLUR:    blur each level, across and then down (see `BLUR_FRAGMENT`).
//   3. COMBINE: the sharp picture plus every blurred level, added on top.
//
// Only what is drawn into the scene glows. Small text (the HUD, the armory's
// cards) is drawn after this, on the finished picture, so it stays crisp
// (see `Glow::ui`).

// Step 1. Four bilinear taps, each between four pixels: an average of a 4x4
// block, so thin lines shrink smoothly instead of flickering in and out.
//
// Bright-pass: only the part of a colour above `threshold` may glow. The grey
// arena sits below it and stays crisp; players, bolts and explosions are above
// it and bleed light. Neon works through selective brightness: if every tile
// glows, nothing stands out. Subtracting per channel also keeps the glow
// coloured: a pale cyan tube (0.5, 1.0, 1.0) glows pure cyan.
pub const DOWN_FRAGMENT: &str = "#version 100
precision mediump float;
varying vec2 uv;
uniform sampler2D Texture;
uniform vec2 texel; // one pixel of the source, in uv units
uniform float threshold;

vec3 bright(vec2 p) {
    return max(texture2D(Texture, p).rgb - threshold, 0.0);
}

void main() {
    vec3 c = bright(uv + texel * vec2(-1.0, -1.0)) + bright(uv + texel * vec2(1.0, -1.0))
           + bright(uv + texel * vec2(-1.0, 1.0)) + bright(uv + texel * vec2(1.0, 1.0));
    gl_FragColor = vec4(c * 0.25 / (1.0 - threshold), 1.0);
}
";

// Step 2. A Gaussian blur along one direction (`step` is one pixel across or
// one pixel down). Blurring across and then down equals a full round blur at
// a fraction of the cost, because the Gaussian is "separable". Nine pixels
// are read with five taps: a tap between two pixels makes the GPU blend them,
// and the offsets and weights are chosen so the blend is exactly the
// Gaussian's.
pub const BLUR_FRAGMENT: &str = "#version 100
precision mediump float;
varying vec2 uv;
uniform sampler2D Texture;
uniform vec2 step;

void main() {
    vec3 c = texture2D(Texture, uv).rgb * 0.2270270;
    c += (texture2D(Texture, uv + step * 1.3846154).rgb + texture2D(Texture, uv - step * 1.3846154).rgb) * 0.3162162;
    c += (texture2D(Texture, uv + step * 3.2307692).rgb + texture2D(Texture, uv - step * 3.2307692).rgb) * 0.0702703;
    gl_FragColor = vec4(c, 1.0);
}
";

// Step 3. Near is the tight glow that hugs a tube, far the wide halo. The
// halo is a very faint, very smooth gradient, and 256 levels per channel show
// as rings in it ("banding"); a tiny per-pixel noise ("dithering") hides them.
pub const COMBINE_FRAGMENT: &str = "#version 100
precision mediump float;
varying vec2 uv;
uniform sampler2D Texture;
uniform sampler2D Near;
uniform sampler2D Far;

void main() {
    vec3 sharp = texture2D(Texture, uv).rgb;
    vec3 glow = texture2D(Near, uv).rgb * 0.75 + texture2D(Far, uv).rgb * 0.55;
    float noise = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453) - 0.5;
    gl_FragColor = vec4(sharp + glow + noise / 255.0, 1.0);
}
";
