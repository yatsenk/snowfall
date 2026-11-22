//! GPU simulation: owns the buffers and runs the compute kernels from sim.wgsl.
//! The CPU only uploads the surface list (when it changes) and a small uniform per frame.

use crate::gpu::Gpu;
use crate::surfaces::{Segment, Slots};
use bytemuck::{Pod, Zeroable};

pub const MAX_SLOTS: usize = 64;
pub const BIN: f32 = 4.0;
const FLAKES: u32 = 700;
const DEBRIS: u32 = 4096; // power of two, so the spawn counter wraps cleanly
const PARTICLE_SIZE: u64 = 32;
const DEPOSITS: u64 = 8192; // FIFO of snow deposits (see sim.wgsl)
const DEPOSIT_SIZE: u64 = 12;
const POP_BATCH: u32 = 1024; // max deposits expired per frame
const WORKGROUP: u32 = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Params {
    screen: [f32; 2],
    dt: f32,
    time: f32,
    frame: u32,
    flake_count: u32,
    pub particle_count: u32,
    pub nodes: u32,
    pub slot_count: u32,
    bin: f32,
    clock: f32, // monotonic seconds, never wraps (deposit expiry)
    _pad: u32,
}

pub struct Sim {
    pub params: Params,
    pub params_buf: wgpu::Buffer,
    pub particles: wgpu::Buffer,
    pub surfaces: wgpu::Buffer,
    pub heights: wgpu::Buffer,
    counters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    update: wgpu::ComputePipeline,
    release: wgpu::ComputePipeline,
    pop: wgpu::ComputePipeline,
    advance: wgpu::ComputePipeline,
    init: wgpu::ComputePipeline,
    settle_even: wgpu::ComputePipeline,
    settle_odd: wgpu::ComputePipeline,
    slots: Slots,
    time: f32,
    clock: f32,
}

pub(crate) fn buffer_binding(
    binding: u32,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BufferBindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

pub(crate) fn bind_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource: buffer.as_entire_binding() }
}

impl Sim {
    pub fn new(gpu: &Gpu, width: u32, height: u32) -> Self {
        let device = &gpu.device;
        let nodes = (width as f32 / BIN).ceil() as u32 + 2;
        let particle_count = FLAKES + DEBRIS;
        let slots = Slots::new(width as f32, height as f32);

        let params = Params {
            screen: [width as f32, height as f32],
            dt: 0.0,
            time: 0.0,
            frame: 0,
            flake_count: FLAKES,
            particle_count,
            nodes,
            slot_count: slots.used(),
            bin: BIN,
            clock: 0.0,
            _pad: 0,
        };

        let buffer = |size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor { label: None, size, usage, mapped_at_creation: false })
        };
        use wgpu::BufferUsages as U;
        let params_buf = buffer(std::mem::size_of::<Params>() as u64, U::UNIFORM | U::COPY_DST);
        let particles = buffer(particle_count as u64 * PARTICLE_SIZE, U::STORAGE | U::VERTEX);
        let surfaces = buffer((MAX_SLOTS * std::mem::size_of::<crate::surfaces::Surface>()) as u64, U::STORAGE | U::COPY_DST);
        let heights = buffer(MAX_SLOTS as u64 * nodes as u64 * 4, U::STORAGE);
        // [spawn, tail, head, popped]
        let counters = buffer(16, U::STORAGE | U::COPY_DST);
        let deposits = buffer(DEPOSITS * DEPOSIT_SIZE, U::STORAGE);

        use wgpu::BufferBindingType as B;
        let cs = wgpu::ShaderStages::COMPUTE;
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                buffer_binding(0, cs, B::Uniform),
                buffer_binding(1, cs, B::Storage { read_only: false }),
                buffer_binding(2, cs, B::Storage { read_only: true }),
                buffer_binding(3, cs, B::Storage { read_only: false }),
                buffer_binding(4, cs, B::Storage { read_only: false }),
                buffer_binding(5, cs, B::Storage { read_only: false }),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[
                bind_entry(0, &params_buf),
                bind_entry(1, &particles),
                bind_entry(2, &surfaces),
                bind_entry(3, &heights),
                bind_entry(4, &counters),
                bind_entry(5, &deposits),
            ],
        });

        let source = [include_str!("common.wgsl"), include_str!("sim.wgsl")].concat();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = |entry_point: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: Some(&layout),
                module: &module,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };

        let mut sim = Self {
            params,
            params_buf,
            particles,
            surfaces,
            heights,
            counters,
            bind_group,
            update: pipeline("update_particles"),
            release: pipeline("release_snow"),
            pop: pipeline("pop_deposits"),
            advance: pipeline("advance_head"),
            init: pipeline("init_world"),
            settle_even: pipeline("settle_even"),
            settle_odd: pipeline("settle_odd"),
            slots,
            time: 0.0,
            clock: 0.0,
        };
        gpu.queue.write_buffer(&sim.surfaces, 0, bytemuck::cast_slice(sim.slots.gpu()));
        sim.reset(gpu);
        sim
    }

    /// Start over: flakes above the screen, no debris, no drifts.
    pub fn reset(&mut self, gpu: &Gpu) {
        gpu.queue.write_buffer(&self.counters, 0, &[0u8; 16]);
        self.next_frame(gpu);
        let threads = self.params.particle_count.max(MAX_SLOTS as u32 * self.params.nodes);
        self.submit(gpu, |s, pass| s.dispatch(pass, &s.init, threads));
    }

    /// Called whenever the window list is refreshed. Cheap when nothing moved;
    /// snow on vanished surfaces is released as falling debris.
    pub fn set_surfaces(&mut self, gpu: &Gpu, segments: &[Segment]) {
        if !self.slots.update(segments) {
            return;
        }
        self.params.slot_count = self.slots.used();
        gpu.queue.write_buffer(&self.surfaces, 0, bytemuck::cast_slice(self.slots.gpu()));
        self.next_frame(gpu);
        let threads = self.params.slot_count * self.params.nodes;
        self.submit(gpu, |s, pass| s.dispatch(pass, &s.release, threads));
    }

    /// Record this frame's physics into `enc`.
    pub fn encode(&mut self, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, dt: f32) {
        self.params.dt = dt;
        self.time = (self.time + dt) % 3600.0;
        self.params.time = self.time;
        self.clock += dt;
        self.params.clock = self.clock;
        self.next_frame(gpu);

        let mut pass = enc.begin_compute_pass(&Default::default());
        pass.set_bind_group(0, &self.bind_group, &[]);
        self.dispatch(&mut pass, &self.update, self.params.particle_count);
        self.dispatch(&mut pass, &self.pop, POP_BATCH); // oldest deposits expire (FIFO)
        self.dispatch(&mut pass, &self.advance, 1);
        let pairs = self.params.slot_count * (self.params.nodes / 2);
        self.dispatch(&mut pass, &self.settle_even, pairs);
        self.dispatch(&mut pass, &self.settle_odd, pairs);
    }

    /// New random seed + upload the uniform.
    fn next_frame(&mut self, gpu: &Gpu) {
        self.params.frame = self.params.frame.wrapping_add(1);
        gpu.queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&self.params));
    }

    fn dispatch(&self, pass: &mut wgpu::ComputePass, pipeline: &wgpu::ComputePipeline, threads: u32) {
        if threads > 0 {
            pass.set_pipeline(pipeline);
            pass.dispatch_workgroups(threads.div_ceil(WORKGROUP), 1, 1);
        }
    }

    /// One-off compute submission (reset / release), ordered with the frame's uploads.
    fn submit(&self, gpu: &Gpu, f: impl FnOnce(&Self, &mut wgpu::ComputePass)) {
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_bind_group(0, &self.bind_group, &[]);
            f(self, &mut pass);
        }
        gpu.queue.submit([enc.finish()]);
    }
}
