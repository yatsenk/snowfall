//! Platform-independent part: GPU snow simulation (compute) and instanced rendering.
//! The platform crate supplies the wgpu surface and the list of window rectangles.

mod gpu;
mod render;
mod sim;
mod surfaces;

pub use surfaces::Rect;

use std::error::Error;

pub struct Snowfall {
    gpu: gpu::Gpu,
    sim: sim::Sim,
    draw: render::Draw,
    segments: Vec<surfaces::Segment>,
    height: f32,
}

impl Snowfall {
    pub fn new(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Result<Self, Box<dyn Error>> {
        let gpu = gpu::Gpu::new(instance, surface, width, height)?;
        let sim = sim::Sim::new(&gpu, width, height);
        let draw = render::Draw::new(&gpu, &sim);
        Ok(Self { gpu, sim, draw, segments: Vec::new(), height: height as f32 })
    }

    /// Window rectangles in overlay coordinates, ordered front-to-back (z-order).
    /// Snow lies on their top edges, is hidden behind them, and falls down when one disappears.
    pub fn set_windows(&mut self, rects: &[Rect]) {
        surfaces::top_edges(rects, self.height, &mut self.segments);
        self.sim.set_surfaces(&self.gpu, &self.segments);
        self.draw.set_occluders(&self.gpu, rects);
    }

    /// Start over: flakes fall from the top again, drifts are cleared.
    pub fn reset(&mut self) {
        self.sim.reset(&self.gpu);
    }

    /// Advance the simulation by `dt` seconds and draw one frame (blocks until vsync).
    pub fn tick(&mut self, dt: f32) {
        let frame = match self.gpu.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                self.gpu.surface.configure(&self.gpu.device, &self.gpu.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        self.sim.encode(&self.gpu, &mut enc, dt);
        self.draw.encode(&mut enc, &view, &self.sim);
        self.gpu.queue.submit([enc.finish()]);
        frame.present();
    }
}
