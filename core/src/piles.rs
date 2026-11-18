//! Snow drifts: one height profile per surface (window top edge, taskbar, screen bottom).
//! Heights are stored at nodes spaced `BIN` px apart on a global x grid, so a profile
//! survives when the surface list is rebuilt.

use crate::renderer::{PileInstance, MAX_PILE_INSTANCES};
use crate::surfaces::Segment;

pub const BIN: f32 = 4.0;
pub const MAX_H: f32 = 14.0;
/// Max height difference between neighbouring nodes (angle of repose).
const SLOPE: f32 = 2.0;
const SETTLE_SPAN: usize = 6;
const SETTLE_PASSES: usize = 3;

struct Pile {
    y: f32,
    x0: f32,
    x1: f32,
    /// Global grid index of `h[0]`.
    first: i32,
    h: Vec<f32>,
}

impl Pile {
    fn height(&self, x: f32) -> f32 {
        let t = x / BIN - self.first as f32;
        let i = (t.floor().max(0.0) as usize).min(self.h.len() - 2);
        let k = (t - i as f32).clamp(0.0, 1.0);
        self.h[i] + (self.h[i + 1] - self.h[i]) * k
    }

    fn add(&mut self, x: f32, amount: f32) {
        let n = self.h.len();
        let k = ((x / BIN).round() as i32 - self.first).clamp(0, n as i32 - 1) as usize;
        self.h[k] += amount * 0.5;
        if k > 0 {
            self.h[k - 1] += amount * 0.25;
        }
        if k + 1 < n {
            self.h[k + 1] += amount * 0.25;
        }
        self.settle(k);
    }

    /// Local relaxation: spill snow downhill wherever the slope is too steep.
    fn settle(&mut self, k: usize) {
        let lo = k.saturating_sub(SETTLE_SPAN);
        let hi = (k + SETTLE_SPAN).min(self.h.len() - 1);
        for _ in 0..SETTLE_PASSES {
            for i in lo..hi {
                let d = self.h[i] - self.h[i + 1];
                if d.abs() > SLOPE {
                    let m = (d.abs() - SLOPE) * 0.5 * d.signum();
                    self.h[i] -= m;
                    self.h[i + 1] += m;
                }
            }
        }
        for v in &mut self.h[lo..=hi] {
            *v = v.min(MAX_H);
        }
    }
}

#[derive(Default)]
pub struct Piles {
    list: Vec<Pile>,
}

impl Piles {
    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// Replace the surfaces, keeping snow on those that stayed in place.
    pub fn set_surfaces(&mut self, segments: &[Segment]) {
        let old = std::mem::take(&mut self.list);
        for s in segments {
            let first = (s.x0 / BIN).ceil() as i32;
            let last = (s.x1 / BIN).floor() as i32;
            if last <= first {
                continue;
            }
            let mut h = vec![0.0; (last - first + 1) as usize];
            for o in old.iter().filter(|o| (o.y - s.y).abs() < 1.5) {
                let from = first.max(o.first);
                let to = last.min(o.first + o.h.len() as i32 - 1);
                for n in from..=to {
                    h[(n - first) as usize] = o.h[(n - o.first) as usize];
                }
            }
            self.list.push(Pile { y: s.y, x0: s.x0, x1: s.x1, first, h });
        }
    }

    /// If a flake moving from `old_bottom` to `bottom` at `x` reaches a drift,
    /// melt it into the pile and return true.
    pub fn land(&mut self, x: f32, old_bottom: f32, bottom: f32, amount: f32) -> bool {
        for p in &mut self.list {
            if old_bottom > p.y || bottom < p.y - MAX_H || x < p.x0 || x > p.x1 {
                continue;
            }
            if bottom >= p.y - p.height(x) {
                p.add(x, amount);
                return true;
            }
        }
        false
    }

    pub fn write_instances(&self, out: &mut Vec<PileInstance>) {
        out.clear();
        for p in &self.list {
            for i in 0..p.h.len() - 1 {
                let (h0, h1) = (p.h[i], p.h[i + 1]);
                if h0 < 0.05 && h1 < 0.05 {
                    continue;
                }
                if out.len() == MAX_PILE_INSTANCES {
                    return;
                }
                out.push(PileInstance { x: (p.first + i as i32) as f32 * BIN, y: p.y, h0, h1 });
            }
        }
    }
}
