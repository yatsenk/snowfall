// Drawing: window cut-outs (stencil), drifts (vertex pulling from the height buffer)
// and particles (instanced soft dots).

@group(0) @binding(3) var<storage, read> heights: array<i32>;
@group(0) @binding(4) var<storage, read> occluders: array<vec4<f32>>;   // window rects (x0, y0, x1, y1)

const PILE_ALPHA: f32 = 0.55;   // drifts stay see-through: this is background snow

var<private> CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(-1.0, 1.0),
    vec2<f32>(-1.0, 1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
);

fn to_clip(p: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(p.x / g.screen.x * 2.0 - 1.0, 1.0 - p.y / g.screen.y * 2.0, 0.0, 1.0);
}

// ---------- windows: only write the stencil, so snow is never drawn over them ----------

@vertex
fn vs_occluder(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> @builtin(position) vec4<f32> {
    let r = occluders[ii];
    let c = CORNERS[vi] * 0.5 + vec2<f32>(0.5);
    return to_clip(mix(r.xy, r.zw, c));
}

@fragment
fn fs_occluder() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}

// ---------- drifts: one quad per pair of neighbouring height nodes ----------

struct PileOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) depth: f32,   // px below the top surface of the drift
};

@vertex
fn vs_pile(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> PileOut {
    var out: PileOut;
    out.clip = vec4<f32>(0.0, 0.0, 0.0, 1.0);   // degenerate = nothing drawn
    out.depth = 0.0;

    let per_slot = g.nodes - 1u;
    let k = ii / per_slot;
    let n = ii % per_slot;
    let s = surfaces[k];
    if (s.state != SURFACE_ACTIVE) { return out; }
    let h0 = f32(heights[k * g.nodes + n]) / FX;
    let h1 = f32(heights[k * g.nodes + n + 1u]) / FX;
    if (h0 < 0.05 && h1 < 0.05) { return out; }

    let c = CORNERS[vi] * 0.5 + vec2<f32>(0.5);
    let h = mix(h0, h1, c.x);
    out.clip = to_clip(vec2<f32>((f32(n) + c.x) * g.bin, s.y - c.y * h));
    out.depth = (1.0 - c.y) * h;
    return out;
}

@fragment
fn fs_pile(in: PileOut) -> @location(0) vec4<f32> {
    // soft, fluffy top edge: fades in over the first couple of pixels
    let a = PILE_ALPHA * (0.25 + 0.75 * smoothstep(0.0, 2.5, in.depth));
    return vec4<f32>(vec3<f32>(0.93, 0.96, 1.0) * a, a);
}

// ---------- snowflakes and debris: a soft dot ----------

struct FlakeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
};

@vertex
fn vs_flake(@builtin(vertex_index) vi: u32, @location(0) data: vec4<f32>) -> FlakeOut {
    // data = (pos.xy, radius, alpha)
    let c = CORNERS[vi];
    var out: FlakeOut;
    out.clip = to_clip(data.xy + c * data.z);
    out.uv = c;
    out.alpha = data.w;
    return out;
}

@fragment
fn fs_flake(in: FlakeOut) -> @location(0) vec4<f32> {
    let a = (1.0 - smoothstep(0.25, 1.0, length(in.uv))) * in.alpha;
    return vec4<f32>(vec3<f32>(0.92, 0.96, 1.0) * a, a); // premultiplied alpha
}
