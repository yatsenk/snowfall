// GPU physics: flakes, debris, drifts. Entry points are dispatched from sim.rs.

@group(0) @binding(1) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(3) var<storage, read_write> heights: array<atomic<i32>>;
@group(0) @binding(4) var<storage, read_write> counters: array<atomic<u32>, 4>;
@group(0) @binding(5) var<storage, read_write> deposits: array<Deposit>;

// Every bit of snow that lands is logged here. The log is a FIFO ring buffer:
// when an entry is SNOW_LIFE seconds old, exactly that snow is removed again.
struct Deposit {
    node: u32,      // slot * nodes + node it landed on
    amount: u32,    // fixed-point (FX) snow mass
    expire: f32,    // g.clock at which it goes away
};

const SPAWN: u32 = 0u;   // debris ring-buffer cursor
const TAIL: u32 = 1u;    // next deposit to be written
const HEAD: u32 = 2u;    // oldest deposit still alive
const POPPED: u32 = 3u;  // deposits expired this frame (head advance)

const TAU: f32 = 6.28318530718;

// --- tuning ---
const FLAKE_ALPHA: f32 = 0.22;      // + up to FLAKE_ALPHA_VAR: keep it subtle, it is background
const FLAKE_ALPHA_VAR: f32 = 0.28;
const AMOUNT_PER_AREA: f32 = 0.7;   // drift height gained per flake r^2
const SLOPE: f32 = 2.0;             // average max height step between neighbour nodes
const SNOW_LIFE: f32 = 40.0;        // seconds until a deposit is removed (oldest first)
const OVERFLOW_MARGIN: u32 = 1024u; // log this close to full: expire oldest early
const GRAVITY: f32 = 700.0;         // px/s^2 for falling debris
const MAX_FALL: f32 = 800.0;        // debris terminal speed, px/s
const DEBRIS_AMOUNT: f32 = 3.0;     // snow mass per debris particle
const MAX_CHUNKS: u32 = 4u;         // max debris particles per height node

// ---------- random ----------

fn rnd(s: ptr<function, u32>) -> f32 {
    *s = hash(*s);
    return f32(*s >> 8u) * (1.0 / 16777216.0);
}

// ---------- drifts ----------

fn node_h(k: u32, n: i32) -> f32 {
    return f32(atomicLoad(&heights[k * g.nodes + u32(n)])) / FX;
}

fn height_at(k: u32, s: Surface, x: f32) -> f32 {
    let t = x / g.bin;
    let i = clamp(i32(floor(t)), first_node(s), last_node(s) - 1);
    return mix(node_h(k, i), node_h(k, i + 1), clamp(t - f32(i), 0.0, 1.0));
}

fn add_snow(k: u32, s: Surface, x: f32, amount: f32) {
    var r = hash(bitcast<u32>(x) ^ hash(g.frame));
    let first = first_node(s);
    let last = last_node(s);
    // uneven deposits (random node, random size) make the surface lumpy, not flat
    let n = clamp(i32(round(x / g.bin + (rnd(&r) - 0.5) * 2.0)), first, last);
    let a = amount * (0.5 + rnd(&r));
    let base = k * g.nodes;
    _ = atomicAdd(&heights[base + u32(n)], i32(a * 0.5 * FX));
    if (n > first) { _ = atomicAdd(&heights[base + u32(n - 1)], i32(a * 0.25 * FX)); }
    if (n < last) { _ = atomicAdd(&heights[base + u32(n + 1)], i32(a * 0.25 * FX)); }

    // log it, so it can be removed when it is the oldest
    let slot = atomicAdd(&counters[TAIL], 1u);
    deposits[slot % arrayLength(&deposits)] = Deposit(base + u32(n), u32(a * FX), g.clock + SNOW_LIFE);
}

// A particle moved from `old_bottom` to `bottom`; if it reached a drift, melt it in.
fn land(x: f32, old_bottom: f32, bottom: f32, amount: f32) -> bool {
    for (var k = 0u; k < g.slot_count; k = k + 1u) {
        let s = surfaces[k];
        if (s.state != SURFACE_ACTIVE || old_bottom > s.y || x < s.x0 || x > s.x1) { continue; }
        if (bottom >= s.y - height_at(k, s, x)) {
            add_snow(k, s, x, amount);
            return true;
        }
    }
    return false;
}

// ---------- particles ----------

fn spawn_flake(i: u32, span: f32) -> Particle {
    var r = hash(i ^ hash(g.frame));
    var f: Particle;
    let k = rnd(&r);
    f.radius = 1.2 + k * k * 2.3;                     // mostly tiny, a few larger
    f.pos = vec2<f32>(rnd(&r) * g.screen.x, -f.radius - rnd(&r) * span);
    f.alpha = FLAKE_ALPHA + rnd(&r) * FLAKE_ALPHA_VAR;
    f.vel = vec2<f32>(rnd(&r) * TAU, 3.0 + rnd(&r) * 6.0);   // (phase, sway)
    f.amount = 0.0;
    f.delay = 0.0;
    return f;
}

fn update_flake(i: u32, s: Particle) -> Particle {
    var f = s;
    let wind = 10.0 * sin(g.time * 0.35 + f.vel.x);
    let drift = f.vel.y * sin(g.time * 1.3 + f.vel.x * 3.0);
    let old_bottom = f.pos.y + f.radius;
    f.pos.x = f.pos.x + (wind + drift) * g.dt;
    f.pos.y = f.pos.y + (14.0 + f.radius * 9.0) * g.dt;

    if (land(f.pos.x, old_bottom, f.pos.y + f.radius, f.radius * f.radius * AMOUNT_PER_AREA)
        || f.pos.y - f.radius > g.screen.y) {
        return spawn_flake(i, 40.0);
    }
    if (f.pos.x < -10.0) {
        f.pos.x = f.pos.x + g.screen.x + 20.0;
    } else if (f.pos.x > g.screen.x + 10.0) {
        f.pos.x = f.pos.x - g.screen.x - 20.0;
    }
    return f;
}

fn update_debris(s: Particle) -> Particle {
    var d = s;
    if (d.radius == 0.0) { return d; }                // dead
    if (d.delay > 0.0) {                              // still clinging to the old spot
        d.delay = d.delay - g.dt;
        return d;
    }
    d.vel.y = min(d.vel.y + GRAVITY * g.dt, MAX_FALL);
    d.vel.x = d.vel.x - d.vel.x * 1.2 * g.dt;         // air drag
    let old_bottom = d.pos.y + d.radius;
    d.pos = d.pos + d.vel * g.dt;
    if (land(d.pos.x, old_bottom, d.pos.y + d.radius, d.amount) || d.pos.y - d.radius > g.screen.y) {
        d.radius = 0.0;
        d.alpha = 0.0;
    }
    return d;
}

@compute @workgroup_size(64)
fn update_particles(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= g.particle_count) { return; }
    if (i < g.flake_count) {
        particles[i] = update_flake(i, particles[i]);
    } else {
        particles[i] = update_debris(particles[i]);
    }
}

// Fresh start: flakes above the screen, no debris, no drifts.
@compute @workgroup_size(64)
fn init_world(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i < g.particle_count) {
        if (i < g.flake_count) {
            particles[i] = spawn_flake(i, g.screen.y);
        } else {
            var dead: Particle;
            particles[i] = dead;
        }
    }
    if (i < arrayLength(&heights)) {
        atomicStore(&heights[i], 0);
    }
}

// Snow on a surface that vanished (window closed/minimized/moved) or on nodes that
// are no longer covered by it is turned into debris that falls down.
@compute @workgroup_size(64)
fn release_snow(@builtin(global_invocation_id) id: vec3<u32>) {
    let idx = id.x;
    if (idx >= g.slot_count * g.nodes) { return; }
    let hv = atomicLoad(&heights[idx]);
    if (hv <= 0) { return; }
    let k = idx / g.nodes;
    let n = i32(idx % g.nodes);
    let s = surfaces[k];
    if (s.state == SURFACE_ACTIVE && n >= first_node(s) && n <= last_node(s)) { return; }

    atomicStore(&heights[idx], 0);
    let h = f32(hv) / FX;
    if (h < 0.05) { return; }
    let count = min(u32(ceil(h / DEBRIS_AMOUNT)), MAX_CHUNKS);
    let amount = h / f32(count);
    let pool = g.particle_count - g.flake_count;
    let base = atomicAdd(&counters[SPAWN], count);  // ring buffer: oldest debris gets overwritten
    var r = hash(idx ^ hash(g.frame));
    for (var j = 0u; j < count; j = j + 1u) {
        var d: Particle;
        d.pos = vec2<f32>((f32(n) + rnd(&r) - 0.5) * g.bin, s.y - h * rnd(&r));
        d.amount = amount;
        d.radius = 0.7 + sqrt(amount) * 0.5;
        d.alpha = 0.45;
        d.vel = vec2<f32>((rnd(&r) - 0.5) * 40.0, 0.0);
        d.delay = rnd(&r) * 0.5;                   // staggered: the drift slides off, not drops
        particles[g.flake_count + (base + j) % pool] = d;
    }
}

// ---------- FIFO: oldest snow goes first ----------

// Remove up to `want` units from one node and return how much was really there.
// (No compare-exchange: the DX12 backend does not implement it. Subtract, and if that
// took more than was available, give the overshoot back. The node ends up >= 0.)
fn take(idx: u32, want: i32) -> i32 {
    let old = atomicAdd(&heights[idx], -want);
    let got = clamp(old, 0, want);
    if (got < want) { _ = atomicAdd(&heights[idx], want - got); }
    return got;
}

// Take the snow from the node it landed on; if slope relaxation has moved some of it
// away, take the rest from the nearest neighbours.
fn remove_snow(node: u32, amount: u32) {
    let row = node / g.nodes;
    let n = i32(node % g.nodes);
    var left = i32(amount);
    for (var j = 0; j < 5 && left > 0; j = j + 1) {
        let off = (j + 1) / 2 * select(1, -1, (j & 1) == 1);   // 0, -1, +1, -2, +2
        let t = n + off;
        if (t < 0 || t >= i32(g.nodes)) { continue; }
        left = left - take(row * g.nodes + u32(t), left);
    }
}

// Each thread looks at one entry from the head of the log. Entries are ordered by
// time, so the expired ones always form a prefix.
@compute @workgroup_size(64)
fn pop_deposits(@builtin(global_invocation_id) id: vec3<u32>) {
    let head = atomicLoad(&counters[HEAD]);
    let tail = atomicLoad(&counters[TAIL]);
    let idx = head + id.x;
    if (idx >= tail) { return; }
    let ring = arrayLength(&deposits);
    let d = deposits[idx % ring];
    let overflow = tail - idx > ring - OVERFLOW_MARGIN;   // log nearly full: drop oldest early
    if (d.expire > g.clock && !overflow) { return; }
    remove_snow(d.node, d.amount);
    _ = atomicAdd(&counters[POPPED], 1u);
}

@compute @workgroup_size(1)
fn advance_head() {
    let n = atomicLoad(&counters[POPPED]);   // single thread: no exchange needed
    atomicStore(&counters[POPPED], 0u);
    _ = atomicAdd(&counters[HEAD], n);
}

// ---------- spreading ----------

// Spread snow downhill. Two passes (even/odd pairs) keep every pair exclusive.
fn settle(id: u32, parity: u32) {
    let pairs = g.nodes / 2u;
    if (id >= g.slot_count * pairs) { return; }
    let k = id / pairs;
    let s = surfaces[k];
    if (s.state != SURFACE_ACTIVE) { return; }
    let i = i32((id % pairs) * 2u + parity);
    if (i < first_node(s) || i + 1 > last_node(s)) { return; }

    let ia = k * g.nodes + u32(i);
    let a = atomicLoad(&heights[ia]);
    let b = atomicLoad(&heights[ia + 1u]);

    // every pair has its own stable repose angle: the drift stays bumpy
    let jitter = f32(hash(ia)) * (1.0 / 4294967296.0);
    let limit = i32(SLOPE * FX * (0.4 + 1.2 * jitter));
    let d = a - b;
    var m = 0;
    if (abs(d) > limit) { m = (abs(d) - limit) / 2 * sign(d); }

    let cap = i32(MAX_H * FX);
    let na = clamp(a - m, 0, cap);
    let nb = clamp(b + m, 0, cap);
    if (na != a) { atomicStore(&heights[ia], na); }
    if (nb != b) { atomicStore(&heights[ia + 1u], nb); }
}

@compute @workgroup_size(64)
fn settle_even(@builtin(global_invocation_id) id: vec3<u32>) { settle(id.x, 0u); }

@compute @workgroup_size(64)
fn settle_odd(@builtin(global_invocation_id) id: vec3<u32>) { settle(id.x, 1u); }
