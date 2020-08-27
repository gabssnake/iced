//! Draw meshes of triangles.
use crate::{settings, Transformation};
use iced_graphics::layer;
use std::mem;
use zerocopy::AsBytes;

pub use iced_graphics::triangle::{Mesh2D, Vertex2D};

mod msaa;

const UNIFORM_BUFFER_SIZE: usize = 50;
const VERTEX_BUFFER_SIZE: usize = 10_000;
const INDEX_BUFFER_SIZE: usize = 10_000;

#[derive(Debug)]
pub(crate) struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    blit: Option<msaa::Blit>,
    constants_layout: wgpu::BindGroupLayout,
    constants: wgpu::BindGroup,
    uniforms_buffer: Buffer<Uniforms>,
    vertex_buffer: Buffer<Vertex2D>,
    index_buffer: Buffer<u32>,
}

#[derive(Debug)]
struct Buffer<T> {
    raw: wgpu::Buffer,
    size: usize,
    usage: wgpu::BufferUsage,
    _type: std::marker::PhantomData<T>,
}

impl<T> Buffer<T> {
    pub fn new(
        device: &wgpu::Device,
        size: usize,
        usage: wgpu::BufferUsage,
    ) -> Self {
        let raw = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (std::mem::size_of::<T>() * size) as u64,
            usage,
            mapped_at_creation: false,
        });

        Buffer {
            raw,
            size,
            usage,
            _type: std::marker::PhantomData,
        }
    }

    pub fn expand(&mut self, device: &wgpu::Device, size: usize) -> bool {
        let needs_resize = self.size < size;

        if needs_resize {
            self.raw = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: (std::mem::size_of::<T>() * size) as u64,
                usage: self.usage,
                mapped_at_creation: false,
            });

            self.size = size;
        }

        needs_resize
    }
}

impl Pipeline {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        antialiasing: Option<settings::Antialiasing>,
    ) -> Pipeline {
        let constants_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::UniformBuffer {
                        dynamic: true,
                        min_binding_size: wgpu::BufferSize::new(
                            mem::size_of::<Uniforms>() as u64,
                        ),
                    },
                    count: None,
                }],
            });

        let constants_buffer = Buffer::new(
            device,
            UNIFORM_BUFFER_SIZE,
            wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        );

        let constant_bind_group =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &constants_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(
                        constants_buffer
                            .raw
                            .slice(0..std::mem::size_of::<Uniforms>() as u64),
                    ),
                }],
            });

        let layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                push_constant_ranges: &[],
                bind_group_layouts: &[&constants_layout],
            });

        let vs_module = device.create_shader_module(wgpu::include_spirv!(
            "shader/triangle.vert.spv"
        ));

        let fs_module = device.create_shader_module(wgpu::include_spirv!(
            "shader/triangle.frag.spv"
        ));

        let pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&layout),
                vertex_stage: wgpu::ProgrammableStageDescriptor {
                    module: &vs_module,
                    entry_point: "main",
                },
                fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                    module: &fs_module,
                    entry_point: "main",
                }),
                rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                    front_face: wgpu::FrontFace::Cw,
                    cull_mode: wgpu::CullMode::None,
                    ..Default::default()
                }),
                primitive_topology: wgpu::PrimitiveTopology::TriangleList,
                color_states: &[wgpu::ColorStateDescriptor {
                    format,
                    color_blend: wgpu::BlendDescriptor {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha_blend: wgpu::BlendDescriptor {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    write_mask: wgpu::ColorWrite::ALL,
                }],
                depth_stencil_state: None,
                vertex_state: wgpu::VertexStateDescriptor {
                    index_format: wgpu::IndexFormat::Uint32,
                    vertex_buffers: &[wgpu::VertexBufferDescriptor {
                        stride: mem::size_of::<Vertex2D>() as u64,
                        step_mode: wgpu::InputStepMode::Vertex,
                        attributes: &[
                            // Position
                            wgpu::VertexAttributeDescriptor {
                                shader_location: 0,
                                format: wgpu::VertexFormat::Float2,
                                offset: 0,
                            },
                            // Color
                            wgpu::VertexAttributeDescriptor {
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float4,
                                offset: 4 * 2,
                            },
                        ],
                    }],
                },
                sample_count: u32::from(
                    antialiasing.map(|a| a.sample_count()).unwrap_or(1),
                ),
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

        Pipeline {
            pipeline,
            blit: antialiasing.map(|a| msaa::Blit::new(device, format, a)),
            constants_layout,
            constants: constant_bind_group,
            uniforms_buffer: constants_buffer,
            vertex_buffer: Buffer::new(
                device,
                VERTEX_BUFFER_SIZE,
                wgpu::BufferUsage::VERTEX | wgpu::BufferUsage::COPY_DST,
            ),
            index_buffer: Buffer::new(
                device,
                INDEX_BUFFER_SIZE,
                wgpu::BufferUsage::INDEX | wgpu::BufferUsage::COPY_DST,
            ),
        }
    }

    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        staging_belt: &mut wgpu::util::StagingBelt,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        target_width: u32,
        target_height: u32,
        transformation: Transformation,
        scale_factor: f32,
        meshes: &[layer::Mesh<'_>],
    ) {
        // This looks a bit crazy, but we are just counting how many vertices
        // and indices we will need to handle.
        // TODO: Improve readability
        let (total_vertices, total_indices) = meshes
            .iter()
            .map(|layer::Mesh { buffers, .. }| {
                (buffers.vertices.len(), buffers.indices.len())
            })
            .fold((0, 0), |(total_v, total_i), (v, i)| {
                (total_v + v, total_i + i)
            });

        // Then we ensure the current buffers are big enough, resizing if
        // necessary
        let _ = self.vertex_buffer.expand(device, total_vertices);
        let _ = self.index_buffer.expand(device, total_indices);

        // If the uniforms buffer is resized, then we need to recreate its
        // bind group.
        if self.uniforms_buffer.expand(device, meshes.len()) {
            self.constants =
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.constants_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(
                            self.uniforms_buffer.raw.slice(
                                0..std::mem::size_of::<Uniforms>() as u64,
                            ),
                        ),
                    }],
                });
        }

        let mut uniforms: Vec<Uniforms> = Vec::with_capacity(meshes.len());
        let mut offsets: Vec<(
            wgpu::BufferAddress,
            wgpu::BufferAddress,
            usize,
        )> = Vec::with_capacity(meshes.len());
        let mut last_vertex = 0;
        let mut last_index = 0;

        // We upload everything upfront
        for mesh in meshes {
            let transform = (transformation
                * Transformation::translate(mesh.origin.x, mesh.origin.y))
            .into();

            let vertices = bytemuck::cast_slice(&mesh.buffers.vertices);
            let indices = bytemuck::cast_slice(&mesh.buffers.indices);

            if let Some(vertices_size) =
                wgpu::BufferSize::new(vertices.len() as u64)
            {
                if let Some(indices_size) =
                    wgpu::BufferSize::new(indices.len() as u64)
                {
                    {
                        let mut vertex_buffer = staging_belt.write_buffer(
                            encoder,
                            &self.vertex_buffer.raw,
                            (std::mem::size_of::<Vertex2D>() * last_vertex)
                                as u64,
                            vertices_size,
                            device,
                        );

                        vertex_buffer.copy_from_slice(vertices);
                    }

                    {
                        let mut index_buffer = staging_belt.write_buffer(
                            encoder,
                            &self.index_buffer.raw,
                            (std::mem::size_of::<u32>() * last_index) as u64,
                            indices_size,
                            device,
                        );

                        index_buffer.copy_from_slice(indices);
                    }

                    uniforms.push(transform);
                    offsets.push((
                        last_vertex as u64,
                        last_index as u64,
                        mesh.buffers.indices.len(),
                    ));

                    last_vertex += mesh.buffers.vertices.len();
                    last_index += mesh.buffers.indices.len();
                }
            }
        }

        let uniforms = uniforms.as_bytes();

        if let Some(uniforms_size) =
            wgpu::BufferSize::new(uniforms.len() as u64)
        {
            let mut uniforms_buffer = staging_belt.write_buffer(
                encoder,
                &self.uniforms_buffer.raw,
                0,
                uniforms_size,
                device,
            );

            uniforms_buffer.copy_from_slice(uniforms);
        }

        {
            let (attachment, resolve_target, load) =
                if let Some(blit) = &mut self.blit {
                    let (attachment, resolve_target) =
                        blit.targets(device, target_width, target_height);

                    (
                        attachment,
                        Some(resolve_target),
                        wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                    )
                } else {
                    (target, None, wgpu::LoadOp::Load)
                };

            let mut render_pass =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[
                        wgpu::RenderPassColorAttachmentDescriptor {
                            attachment,
                            resolve_target,
                            ops: wgpu::Operations { load, store: true },
                        },
                    ],
                    depth_stencil_attachment: None,
                });

            render_pass.set_pipeline(&self.pipeline);

            for (i, (vertex_offset, index_offset, indices)) in
                offsets.into_iter().enumerate()
            {
                let clip_bounds = (meshes[i].clip_bounds * scale_factor).snap();

                render_pass.set_scissor_rect(
                    clip_bounds.x,
                    clip_bounds.y,
                    clip_bounds.width,
                    clip_bounds.height,
                );

                render_pass.set_bind_group(
                    0,
                    &self.constants,
                    &[(std::mem::size_of::<Uniforms>() * i) as u32],
                );

                render_pass.set_index_buffer(self.index_buffer.raw.slice(..));

                render_pass
                    .set_vertex_buffer(0, self.vertex_buffer.raw.slice(..));

                render_pass.draw_indexed(
                    index_offset as u32
                        ..(index_offset as usize + indices) as u32,
                    vertex_offset as i32,
                    0..1,
                );
            }
        }

        if let Some(blit) = &mut self.blit {
            blit.draw(encoder, target);
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, AsBytes)]
struct Uniforms {
    transform: [f32; 16],
    // We need to align this to 256 bytes to please `wgpu`...
    // TODO: Be smarter and stop wasting memory!
    _padding_a: [f32; 32],
    _padding_b: [f32; 16],
}

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            transform: *Transformation::identity().as_ref(),
            _padding_a: [0.0; 32],
            _padding_b: [0.0; 16],
        }
    }
}

impl From<Transformation> for Uniforms {
    fn from(transformation: Transformation) -> Uniforms {
        Self {
            transform: transformation.into(),
            _padding_a: [0.0; 32],
            _padding_b: [0.0; 16],
        }
    }
}
