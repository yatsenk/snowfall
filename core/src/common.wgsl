// Shared by the simulation (compute) and the drawing (render) modules.

struct Params {
    screen: vec2<f32>,
    dt: f32,
    time: f32,
    frame: u32,
    flake_count: u32,
    particle_count: u32,
    nodes: u32,        // height nodes per surface slot
    slot_count: u32,   // slots in use (active or being released)
    bin: f32,          // distance between height nodes, px
    _pad0: u32,
    _pad1: u32,
};

// One 32-byte record for both snowflakes (index < flake_count) and falling debris.
// The vertex shader reads the first 16 bytes as one vec4: (pos.xy, radius, alpha).
struct Particle {
    pos: vec2<f32>,
    radius: f32,       // 0 = dead debris, draws nothing
    alpha: f32,
    vel: vec2<f32>,    // flake: (phase, sway); debris: velocity
    amount: f32,       // debris: mass of snow it carries
    delay: f32,        // debris: seconds before it lets go of the drift
};

// A place where snow can lie: top edge of a window, the taskbar, the screen bottom.
struct Surface {
    y: f32,
    x0: f32,
    x1: f32,
    state: u32,
};

const SURFACE_ACTIVE: u32 = 1u;
const FX: f32 = 4096.0;    // drift heights are fixed-point: atomics need integers
const MAX_H: f32 = 14.0;   // hard cap on drift height, px (melting keeps it well below)

@group(0) @binding(0) var<uniform> g: Params;
@group(0) @binding(2) var<storage, read> surfaces: array<Surface>;

fn first_node(s: Surface) -> i32 { return i32(ceil(s.x0 / g.bin)); }
fn last_node(s: Surface) -> i32 { return i32(floor(s.x1 / g.bin)); }
