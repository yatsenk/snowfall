struct Globals {
    screen: vec2<f32>,
    bin: f32,
    _pad: f32,
};
@group(0) @binding(0) var<uniform> g: Globals;

const TAU: f32 = 6.28318530718;
const SECTOR: f32 = 1.04719755120; // TAU / 6

var<private> CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
    vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
);

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
    @location(2) radius: f32,
    @location(3) angle: f32,
    @location(4) variant: f32,
};

// ---------- drifts: quad between two height nodes ----------

struct PileOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) shade: f32,
};

@vertex
fn vs_pile(@builtin(vertex_index) vi: u32, @location(0) p: vec4<f32>) -> PileOut {
    // p = (x, base y, height at left node, height at right node)
    let c = CORNERS[vi] * 0.5 + vec2<f32>(0.5);
    let h = mix(p.z, p.w, c.x);
    let x = p.x + c.x * g.bin;
    let y = p.y - c.y * h;
    var out: PileOut;
    out.clip = vec4<f32>(x / g.screen.x * 2.0 - 1.0, 1.0 - y / g.screen.y * 2.0, 0.0, 1.0);
    out.shade = c.y;
    return out;
}

@fragment
fn fs_pile(in: PileOut) -> @location(0) vec4<f32> {
    let k = 0.82 + 0.18 * in.shade; // slightly darker towards the base
    let a = 0.95;
    return vec4<f32>(vec3<f32>(0.93, 0.96, 1.0) * k * a, a);
}

// ---------- snowflakes ----------

@vertex
fn vs_flake(
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec2<f32>,
    @location(1) radius: f32,
    @location(2) alpha: f32,
    @location(3) angle: f32,
    @location(4) variant: f32,
) -> VsOut {
    let c = CORNERS[vi];
    let p = pos + c * radius;
    var out: VsOut;
    out.clip = vec4<f32>(p.x / g.screen.x * 2.0 - 1.0, 1.0 - p.y / g.screen.y * 2.0, 0.0, 1.0);
    out.uv = c;
    out.alpha = alpha;
    out.radius = radius;
    out.angle = angle;
    out.variant = variant;
    return out;
}

fn seg(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}

@fragment
fn fs_flake(in: VsOut) -> @location(0) vec4<f32> {
    // rotate the quad coordinates
    let cs = cos(in.angle);
    let sn = sin(in.angle);
    let p = vec2<f32>(in.uv.x * cs - in.uv.y * sn, in.uv.x * sn + in.uv.y * cs);

    // fold into one half-wedge: 6-fold rotational + mirror symmetry
    let r = length(p);
    var a = atan2(p.y, p.x) + SECTOR * 0.5;
    a = a - floor(a / SECTOR) * SECTOR;
    a = abs(a - SECTOR * 0.5);
    let q = r * vec2<f32>(cos(a), sin(a));

    // keep lines at least ~1px wide so small flakes stay visible
    let px = 1.0 / max(in.radius, 1.0);
    let w = max(0.07, px * 0.75);
    let aa = px * 0.6;

    // variant 0..1 shifts branch placement/length so flakes differ
    let b1 = 0.30 + 0.12 * in.variant;
    let b2 = 0.58 + 0.10 * in.variant;
    let bl = 0.24 + 0.08 * in.variant;
    let dir = vec2<f32>(cos(1.0472), sin(1.0472)); // 60 degrees

    var d = seg(q, vec2<f32>(0.0), vec2<f32>(0.95, 0.0));
    d = min(d, seg(q, vec2<f32>(b1, 0.0), vec2<f32>(b1, 0.0) + dir * bl));
    d = min(d, seg(q, vec2<f32>(b2, 0.0), vec2<f32>(b2, 0.0) + dir * bl * 0.8));
    // small tip fork
    d = min(d, seg(q, vec2<f32>(0.85, 0.0), vec2<f32>(0.85, 0.0) + dir * 0.12));

    let crystal = 1.0 - smoothstep(w - aa, w + aa, d);
    let core = 1.0 - smoothstep(0.0, 0.32, r);
    let a_out = clamp(max(crystal, core * 0.7), 0.0, 1.0) * in.alpha;

    // premultiplied alpha, slight blue tint
    return vec4<f32>(vec3<f32>(0.92, 0.96, 1.0) * a_out, a_out);
}
