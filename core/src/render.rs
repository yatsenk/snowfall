//! Drawing, instanced draw calls fed straight from the GPU simulation buffers:
//! 1. window rectangles -> stencil only (snow is cut out wherever a window is, so it
//!    reads as being behind the windows even though the overlay is on top),
//! 2. drifts, 3. snowflakes and debris.

use crate::gpu::Gpu;
use crate::sim::{bind_entry, buffer_binding, Sim};
use crate::surfaces::Rect;

const MAX_OCCLUDERS: usize = 128;
// strands * arcs * bulbs, see GARLAND_* in render.wgsl
const GARLAND_INSTANCES: u32 = 2 * 24 * 14;
const PARTICLE_ATTRS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x4];

pub struct Draw {
    occluder_pipeline: wgpu::RenderPipeline,
    garland_pipeline: wgpu::RenderPipeline,
    pile_pipeline: wgpu::RenderPipeline,
    flake_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    occluder_buf: wgpu::Buffer,
    occluders: u32,
    _stencil: wgpu::Texture,
    stencil_view: wgpu::TextureView,
}

fn stencil_state(compare: wgpu::CompareFunction, pass_op: wgpu::StencilOperation) -> wgpu::DepthStencilState {
    let face = wgpu::StencilFaceState {
        compare,
        fail_op: wgpu::StencilOperation::Keep,
        depth_fail_op: wgpu::StencilOperation::Keep,
        pass_op,
    };
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Stencil8,
        depth_write_enabled: false,
        depth_compare: wgpu::CompareFunction::Always,
        stencil: wgpu::StencilState { front: face, back: face, read_mask: 0xff, write_mask: 0xff },
        bias: wgpu::DepthBiasState::default(),
    }
}

impl Draw {
    pub fn new(gpu: &Gpu, sim: &Sim) -> Self {
        let device = &gpu.device;

        let occluder_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (MAX_OCCLUDERS * std::mem::size_of::<Rect>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let stencil = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: gpu.config.width, height: gpu.config.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Stencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let stencil_view = stencil.create_view(&Default::default());

        use wgpu::BufferBindingType as B;
        let vs = wgpu::ShaderStages::VERTEX;
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                buffer_binding(0, vs, B::Uniform),
                buffer_binding(2, vs, B::Storage { read_only: true }),
                buffer_binding(3, vs, B::Storage { read_only: true }),
                buffer_binding(4, vs, B::Storage { read_only: true }),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[
                bind_entry(0, &sim.params_buf),
                bind_entry(2, &sim.surfaces),
                bind_entry(3, &sim.heights),
                bind_entry(4, &occluder_buf),
            ],
        });

        let source = [include_str!("common.wgsl"), include_str!("render.wgsl")].concat();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = |vs: &str,
                        fs: &str,
                        buffers: &[wgpu::VertexBufferLayout],
                        writes: wgpu::ColorWrites,
                        depth_stencil: wgpu::DepthStencilState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: gpu.config.format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: writes,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth_stencil),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };

        use wgpu::{CompareFunction as C, StencilOperation as Op};
        let all = wgpu::ColorWrites::ALL;
        Self {
            // windows: write 1 into the stencil, no colour
            occluder_pipeline: pipeline(
                "vs_occluder",
                "fs_occluder",
                &[],
                wgpu::ColorWrites::empty(),
                stencil_state(C::Always, Op::Replace),
            ),
            // garland and snow: only where the stencil is still 0
            garland_pipeline: pipeline("vs_garland", "fs_garland", &[], all, stencil_state(C::Equal, Op::Keep)),
            pile_pipeline: pipeline("vs_pile", "fs_pile", &[], all, stencil_state(C::Equal, Op::Keep)),
            flake_pipeline: pipeline(
                "vs_flake",
                "fs_flake",
                &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &PARTICLE_ATTRS,
                }],
                all,
                stencil_state(C::Equal, Op::Keep),
            ),
            bind_group,
            occluder_buf,
            occluders: 0,
            _stencil: stencil,
            stencil_view,
        }
    }

    /// Window rectangles (overlay coordinates) that snow must not be drawn over.
    pub fn set_occluders(&mut self, gpu: &Gpu, rects: &[Rect]) {
        let n = rects.len().min(MAX_OCCLUDERS);
        if n > 0 {
            gpu.queue.write_buffer(&self.occluder_buf, 0, bytemuck::cast_slice(&rects[..n]));
        }
        self.occluders = n as u32;
    }

    pub fn encode(&self, enc: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, sim: &Sim) {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.stencil_view,
                depth_ops: None,
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Discard,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_bind_group(0, &self.bind_group, &[]);

        if self.occluders > 0 {
            pass.set_pipeline(&self.occluder_pipeline);
            pass.set_stencil_reference(1);
            pass.draw(0..6, 0..self.occluders);
        }

        pass.set_stencil_reference(0);
        pass.set_pipeline(&self.garland_pipeline); // behind the snow
        pass.draw(0..12, 0..GARLAND_INSTANCES);
        let p = &sim.params;
        pass.set_pipeline(&self.pile_pipeline); // drifts first, flakes on top
        pass.draw(0..6, 0..p.slot_count * (p.nodes - 1));
        pass.set_pipeline(&self.flake_pipeline);
        pass.set_vertex_buffer(0, sim.particles.slice(..));
        pass.draw(0..6, 0..p.particle_count);
    }
}
