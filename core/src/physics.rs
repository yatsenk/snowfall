use crate::piles::Piles;
use crate::renderer::{Instance, MAX_INSTANCES};

const FLAKE_COUNT: usize = 700;
const _: () = assert!(FLAKE_COUNT <= MAX_INSTANCES);

struct Flake {
    x: f32,
    y: f32,
    radius: f32,
    fall: f32,
    phase: f32,
    sway: f32,
    alpha: f32,
    angle: f32,
    spin: f32,
    variant: f32,
}

pub struct World {
    flakes: Vec<Flake>,
    width: f32,
    height: f32,
    time: f32,
    rng: u32,
}

impl World {
    pub fn new(width: f32, height: f32) -> Self {
        let mut w = Self { flakes: Vec::with_capacity(FLAKE_COUNT), width, height, time: 0.0, rng: 0x9E37_79B9 };
        for _ in 0..FLAKE_COUNT {
            let f = w.new_flake();
            w.flakes.push(f);
        }
        w.reset();
        w
    }

    /// Start the snowfall over: every flake gets a fresh spot above the screen.
    pub fn reset(&mut self) {
        for i in 0..self.flakes.len() {
            let mut f = self.new_flake();
            f.y = -self.rand() * self.height;
            self.flakes[i] = f;
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    fn new_flake(&mut self) -> Flake {
        // mostly small flakes, a few larger ones
        let k = self.rand();
        let radius = 2.0 + k * k * 4.0;
        Flake {
            x: self.rand() * self.width,
            y: -radius - self.rand() * 40.0,
            radius,
            fall: 20.0 + radius * 8.0,
            phase: self.rand() * std::f32::consts::TAU,
            sway: 4.0 + self.rand() * 8.0,
            alpha: 0.65 + self.rand() * 0.35,
            angle: self.rand() * std::f32::consts::TAU,
            spin: (self.rand() - 0.5) * 1.2,
            variant: self.rand(),
        }
    }

    pub fn update(&mut self, dt: f32, piles: &mut Piles) {
        self.time += dt;
        let t = self.time;
        for i in 0..self.flakes.len() {
            let f = &mut self.flakes[i];
            let wind = 10.0 * (t * 0.35 + f.phase).sin();
            let drift = f.sway * (t * 1.3 + f.phase * 3.0).sin();
            let old_bottom = f.y + f.radius;
            f.angle += f.spin * dt;
            f.x += (wind + drift) * dt;
            f.y += f.fall * dt;

            let landed = piles.land(f.x, old_bottom, f.y + f.radius, f.radius * f.radius * 0.4);
            if landed || f.y - f.radius > self.height {
                self.flakes[i] = self.new_flake();
                continue;
            }
            if f.x < -10.0 {
                f.x += self.width + 20.0;
            } else if f.x > self.width + 10.0 {
                f.x -= self.width + 20.0;
            }
        }
    }

    pub fn write_instances(&self, out: &mut Vec<Instance>) {
        out.clear();
        out.extend(self.flakes.iter().filter(|f| f.y + f.radius > 0.0).map(|f| Instance {
            pos: [f.x, f.y],
            radius: f.radius,
            alpha: f.alpha,
            angle: f.angle,
            variant: f.variant,
        }));
    }
}
