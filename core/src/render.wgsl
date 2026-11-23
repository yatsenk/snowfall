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

// ---------- garland: dim red string lights along the top of the screen ----------
// Everything is computed here from the instance index: no buffers, no CPU work.
// Keep GARLAND_MAX_ARCS / GARLAND_BULBS / strand count in sync with render.rs.

const GARLAND_STRANDS: u32 = 2u;
const GARLAND_MAX_ARCS: u32 = 24u;   // arcs per strand (covers screens up to ~8000 px)
const GARLAND_BULBS: u32 = 14u;      // bulbs per arc
const GARLAND_TOP: f32 = 2.0;        // y of the anchor points
const WIRE_HALF: f32 = 0.7;          // half thickness of the wire, px
const BULB_DROP: f32 = 7.0;          // bulbs hang this far under the wire, px
const GLOW_RADIUS: f32 = 24.0;       // size of the glow around a bulb, px

struct Strand {
    span: f32,    // width of one arc
    sag: f32,     // how deep it hangs
    x_off: f32,   // horizontal shift, so the two strands interleave
};

fn strand(i: u32) -> Strand {
    if (i == 0u) { return Strand(480.0, 70.0, 0.0); }
    return Strand(340.0, 44.0, 170.0);
}

fn strand_point(st: Strand, arc: u32, u: f32) -> vec2<f32> {
    let x0 = st.x_off - st.span + f32(arc) * st.span;
    return vec2<f32>(x0 + u * st.span, GARLAND_TOP + st.sag * 4.0 * u * (1.0 - u));
}

fn hash01(v: u32) -> f32 {
    return f32(hash(v) >> 8u) * (1.0 / 16777216.0);
}

// Horror-movie lights: dim, slowly breathing; some bulbs fail in sudden blackouts,
// a few are dead.
fn bulb_light(id: u32) -> f32 {
    let kind = hash01(id * 5u);
    let speed = 0.5 + hash01(id * 5u + 1u) * 0.9;
    let phase = hash01(id * 5u + 2u) * 6.2832;
    if (kind > 0.94) { return 0.0; }                       // dead bulb
    var l = 0.30 + 0.20 * sin(g.time * speed + phase);     // slow dim pulse
    if (kind < 0.22) {                                     // failing bulb
        let slot = u32(floor(g.time * 8.0 + hash01(id * 5u + 3u) * 100.0));
        if (hash01(slot * 31u + id) < 0.4) { l = l * 0.04; }
    }
    return l;
}

struct GarlandOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) kind: f32,    // 0 = wire, 1 = bulb
    @location(2) light: f32,
};

// 12 vertices per instance: 6 for the wire segment, 6 for the bulb under it.
@vertex
fn vs_garland(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> GarlandOut {
    var out: GarlandOut;
    out.clip = vec4<f32>(0.0, 0.0, 0.0, 1.0);   // degenerate = nothing drawn
    out.uv = vec2<f32>(0.0);
    out.kind = 0.0;
    out.light = 0.0;

    let per_strand = GARLAND_MAX_ARCS * GARLAND_BULBS;
    let st = strand(ii / per_strand);
    let r = ii % per_strand;
    let arc = r / GARLAND_BULBS;
    let j = r % GARLAND_BULBS;
    if (st.x_off - st.span + f32(arc) * st.span > g.screen.x) { return out; }   // beyond the screen

    let b = f32(GARLAND_BULBS);
    let p0 = strand_point(st, arc, f32(j) / b);
    let p1 = strand_point(st, arc, f32(j + 1u) / b);
    let c = CORNERS[vi % 6u];
    if (vi < 6u) {
        let d = normalize(p1 - p0);
        let n = vec2<f32>(-d.y, d.x);
        out.clip = to_clip(mix(p0, p1, c.x * 0.5 + 0.5) + n * c.y * WIRE_HALF);
    } else {
        let centre = (p0 + p1) * 0.5 + vec2<f32>(0.0, BULB_DROP);
        out.clip = to_clip(centre + c * GLOW_RADIUS);
        out.uv = c;
        out.kind = 1.0;
        out.light = bulb_light(ii);
    }
    return out;
}

@fragment
fn fs_garland(in: GarlandOut) -> @location(0) vec4<f32> {
    if (in.kind < 0.5) {
        return vec4<f32>(0.165, 0.017, 0.017, 0.55);        // dark red wire
    }
    let d = length(in.uv);
    let core = 1.0 - smoothstep(0.0, 0.12, d);
    let glow = exp(-d * d * 9.0) * (1.0 - smoothstep(0.7, 1.0, d));
    let a = (0.9 * core + 0.45 * glow) * in.light;
    return vec4<f32>(vec3<f32>(1.0, 0.07, 0.04) * a, a);   // premultiplied alpha
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
