use crate::sim::MAX_SLOTS;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// A horizontal stretch of a window's top edge that is actually visible
/// (not covered by a window above it) and can hold snow.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
}

const MIN_SEGMENT_WIDTH: f32 = 8.0;

/// `rects` must be ordered front-to-back.
pub fn top_edges(rects: &[Rect], screen_h: f32, out: &mut Vec<Segment>) {
    out.clear();
    for (i, r) in rects.iter().enumerate() {
        if r.top <= 0.0 || r.top >= screen_h {
            continue;
        }
        let mut spans = vec![(r.left, r.right)];
        for above in &rects[..i] {
            if above.top <= r.top && r.top < above.bottom {
                let mut next = Vec::with_capacity(spans.len() + 1);
                for (a, b) in spans {
                    if above.right <= a || above.left >= b {
                        next.push((a, b));
                        continue;
                    }
                    if a < above.left {
                        next.push((a, above.left));
                    }
                    if above.right < b {
                        next.push((above.right, b));
                    }
                }
                spans = next;
            }
        }
        for (a, b) in spans {
            if b - a >= MIN_SEGMENT_WIDTH {
                out.push(Segment { y: r.top, x0: a, x1: b });
            }
        }
    }
}

const FREE: u32 = 0;
const ACTIVE: u32 = 1;
const RELEASING: u32 = 2; // vanished: its snow is turned into falling debris

/// GPU-side surface record (matches `Surface` in common.wgsl).
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Pod, Zeroable)]
pub struct Surface {
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
    pub state: u32,
}

/// Same surface as before? Same height and at least one edge in place
/// (a window being moved or resized sideways counts as a new surface).
fn same_surface(old: &Surface, new: &Surface) -> bool {
    old.state == ACTIVE
        && (old.y - new.y).abs() < 1.5
        && old.x0 < new.x1
        && old.x1 > new.x0
        && ((old.x0 - new.x0).abs() < 2.0 || (old.x1 - new.x1).abs() < 2.0)
}

/// Stable slot per surface, so a drift keeps its place in the GPU height buffer
/// while the window list is rebuilt. Slot 0 is the bottom of the screen.
pub struct Slots {
    cur: [Surface; MAX_SLOTS],
    gpu: [Surface; MAX_SLOTS],
    width: f32,
}

impl Slots {
    pub fn new(width: f32, height: f32) -> Self {
        let mut cur = [Surface::default(); MAX_SLOTS];
        cur[0] = Surface { y: height, x0: 0.0, x1: width, state: ACTIVE };
        Self { cur, gpu: cur, width }
    }

    /// Match `segments` to slots. Returns false if nothing changed.
    pub fn update(&mut self, segments: &[Segment]) -> bool {
        let mut next = [Surface::default(); MAX_SLOTS];
        next[0] = self.cur[0];
        let mut taken = [false; MAX_SLOTS];
        taken[0] = true;

        let mut fresh = Vec::new();
        for s in segments {
            let c = Surface { y: s.y, x0: s.x0.max(0.0), x1: s.x1.min(self.width), state: ACTIVE };
            if c.x1 - c.x0 < MIN_SEGMENT_WIDTH {
                continue;
            }
            match (1..MAX_SLOTS).find(|&k| !taken[k] && same_surface(&self.cur[k], &c)) {
                Some(k) => {
                    taken[k] = true;
                    next[k] = c;
                }
                None => fresh.push(c),
            }
        }
        // New surfaces only take slots that were free before this update, so a slot
        // being released is never handed straight to another window.
        for c in fresh {
            if let Some(k) = (1..MAX_SLOTS).find(|&k| !taken[k] && self.cur[k].state != ACTIVE) {
                taken[k] = true;
                next[k] = c;
            }
        }

        if next == self.cur {
            return false;
        }
        self.gpu = next;
        for k in 1..MAX_SLOTS {
            if self.cur[k].state == ACTIVE && next[k].state != ACTIVE {
                self.gpu[k] = Surface { state: RELEASING, ..self.cur[k] };
            }
        }
        self.cur = next;
        true
    }

    pub fn gpu(&self) -> &[Surface; MAX_SLOTS] {
        &self.gpu
    }

    /// Number of slots the GPU has to look at (highest non-free index + 1).
    pub fn used(&self) -> u32 {
        self.gpu.iter().rposition(|s| s.state != FREE).map_or(0, |i| i as u32 + 1)
    }
}
