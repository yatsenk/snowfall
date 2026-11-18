//! Drawing: two instanced draw calls, both fed straight from the GPU simulation buffers.

use crate::gpu::Gpu;
use crate::sim::{bind_entry, buffer_binding, Sim};

const PARTICLE_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32, 4 => Float32];

pub struct Draw {
    pile_pipeline: wgpu::RenderPipeline,
    flake_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

impl Draw {
    pub fn new(gpu: &Gpu, sim: &Sim) -> Self {
        let device = &gpu.device;
        use wgpu::BufferBindingType as B;
        let vs = wgpu::ShaderStages::VERTEX;
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                buffer_binding(0, vs, B::Uniform),
                buffer_binding(2, vs, B::Storage { read_only: true }),
                buffer_binding(3, vs, B::Storage { read_only: true }),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[
                bind_entry(0, &sim.params_buf),
                bind_entry(2, &sim.surfaces),
                bind_entry(3, &sim.heights),
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
        let pipeline = |vs: &str, fs: &str, buffers: &[wgpu::VertexBufferLayout]| {
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
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        };

        Self {
            pile_pipeline: pipeline("vs_pile", "fs_pile", &[]),
            flake_pipeline: pipeline(
                "vs_flake",
                "fs_flake",
                &[wgpu::VertexBufferLayout {
                    array_stride: 48,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &PARTICLE_ATTRS,
                }],
            ),
            bind_group,
        }
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
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_bind_group(0, &self.bind_group, &[]);

        let p = &sim.params;
        pass.set_pipeline(&self.pile_pipeline); // drifts first, flakes on top
        pass.draw(0..6, 0..p.slot_count * (p.nodes - 1));
        pass.set_pipeline(&self.flake_pipeline);
        pass.set_vertex_buffer(0, sim.particles.slice(..));
        pass.draw(0..6, 0..p.particle_count);
    }
}
