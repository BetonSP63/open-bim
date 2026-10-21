use crate::{Camera, Scene, ViewState};
use egui_wgpu::{CallbackResources, CallbackTrait, ScreenDescriptor, wgpu};
use std::sync::Arc;
use wgpu::util::DeviceExt;

struct MeshBuffers {
    vertices: wgpu::Buffer,
    count: u32,
    /// Feature edges as a line list (same vertex layout, zero normals).
    edges: wgpu::Buffer,
    edge_count: u32,
}
struct Target {
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    group: wgpu::BindGroup,
    size: [u32; 2],
}
pub struct GpuRenderer {
    pipeline: wgpu::RenderPipeline,
    /// See-through parts: blended, depth tested but not written.
    transparent: wgpu::RenderPipeline,
    edges: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    target: Option<Target>,
    scene: Option<Arc<Scene>>,
    meshes: Vec<MeshBuffers>,
    /// Per mesh: opaque and see-through instance buffers with their counts.
    instances: Vec<[wgpu::Buffer; 2]>,
    counts: Vec<[u32; 2]>,
}
fn bytes(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}
const VERTEX: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
const INSTANCE: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4];
impl GpuRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera + section"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("BIM depth + Lambert"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&uniform_layout)],
            immediate_size: 0,
        });
        let geometry = |label, fragment, topology, blend, depth_write, cull_mode| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 24,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &VERTEX,
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 80,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &INSTANCE,
                        }),
                    ],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let triangles = wgpu::PrimitiveTopology::TriangleList;
        let pipeline = geometry("BIM geometry", "fs", triangles, None, true, None);
        let transparent = geometry(
            "BIM see-through geometry",
            "fs",
            triangles,
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            false,
            // Closed, consistently wound solids: the far side of a see-through
            // part is its own back faces, which would only darken it twice.
            Some(wgpu::Face::Back),
        );
        let edges = geometry(
            "BIM feature edges",
            "fs_edge",
            wgpu::PrimitiveTopology::LineList,
            None,
            false,
            None,
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport composite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&texture_layout)],
            immediate_size: 0,
        });
        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport composite"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            transparent,
            edges,
            composite,
            uniform,
            uniform_group,
            texture_layout,
            sampler,
            target: None,
            scene: None,
            meshes: Vec::new(),
            instances: Vec::new(),
            counts: Vec::new(),
        }
    }
    fn resize(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = |format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("BIM viewport"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let color = texture(
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = texture(
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.target = Some(Target {
            color,
            depth,
            group,
            size,
        });
    }
    fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &ViewportCallback,
        size: [u32; 2],
    ) {
        let scene = &view.scene;
        let camera = &view.camera;
        let state = &view.state;
        self.resize(device, size);
        if self
            .scene
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, scene))
        {
            self.meshes = scene
                .meshes
                .iter()
                .map(|mesh| {
                    let mut vertices = Vec::new();
                    for triangle in &mesh.triangles {
                        let p = triangle.map(|i| mesh.positions[i as usize]);
                        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
                        for position in p {
                            vertices.extend(position.to_array().map(|v| v as f32));
                            vertices.extend(n.to_array().map(|v| v as f32));
                        }
                    }
                    let mut lines = Vec::new();
                    for edge in crate::scene::feature_edges(mesh, 25.0) {
                        for p in edge {
                            lines.extend(p.to_array().map(|v| v as f32));
                            lines.extend([0.0f32; 3]);
                        }
                    }
                    if lines.is_empty() {
                        lines.resize(12, 0.0);
                    }
                    MeshBuffers {
                        count: (vertices.len() / 6) as u32,
                        vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("shared profile mesh"),
                            contents: &bytes(vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                        edge_count: (lines.len() / 6) as u32,
                        edges: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("shared profile edges"),
                            contents: &bytes(lines),
                            usage: wgpu::BufferUsages::VERTEX,
                        }),
                    }
                })
                .collect();
            self.scene = Some(scene.clone());
        }
        let mut batches = vec![[Vec::new(), Vec::new()]; scene.meshes.len()];
        self.counts = vec![[0, 0]; scene.meshes.len()];
        let display = state.display;
        // Pixels per millimetre at a world point (perspective) or constant (ortho).
        let eye = camera.eye();
        let viewport = size[1].max(1) as f64;
        let focal = viewport / (2.0 * (45_f64.to_radians() / 2.0).tan());
        let pixels_per_mm = |p: crate::DVec3| {
            if camera.orthographic {
                viewport / camera.ortho_height.max(1e-9)
            } else {
                focal / (p - eye).length().max(1e-9)
            }
        };
        for instance in &scene.instances {
            if state.hidden.contains(&instance.id) {
                continue;
            }
            let mesh = scene.lod_mesh(instance, pixels_per_mm);
            let selected =
                state.selected == Some(instance.id) || state.selected_ids.contains(&instance.id);
            let look = state.appearance.get(&instance.id);
            let alpha = display.alpha(selected).min(look.map_or(1.0, |a| a.alpha));
            let layer = usize::from(alpha < 1.0);
            let batch = &mut batches[mesh][layer];
            let frame = instance.frame;
            for axis in frame.axes {
                batch.extend(axis.map(|v| v as f32));
                batch.push(0.0);
            }
            batch.extend(
                (crate::DVec3::from_array(frame.origin.0) - scene.origin)
                    .to_array()
                    .map(|v| v as f32),
            );
            batch.push(1.0);
            let [r, g, b, _] = if selected {
                [1.0, 0.42, 0.08, 1.0]
            } else if let Some(color) = look.and_then(|a| a.color) {
                [color[0], color[1], color[2], 1.0]
            } else {
                instance.color
            };
            batch.extend([r, g, b, alpha]);
            self.counts[mesh][layer] += 1;
        }
        let upload = |mut batch: Vec<f32>| {
            if batch.is_empty() {
                batch.resize(20, 0.0);
            }
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("visible BIM instances"),
                contents: &bytes(batch),
                usage: wgpu::BufferUsages::VERTEX,
            })
        };
        self.instances = batches
            .into_iter()
            .map(|[opaque, clear]| [upload(opaque), upload(clear)])
            .collect();
        let mut values = camera
            .matrix(scene.origin, size[0] as f64 / size[1] as f64)
            .to_cols_array()
            .map(|v| v as f32)
            .to_vec();
        for plane in [state.section, state.depth] {
            values.extend(plane.map_or([0.0, 0.0, 0.0, 1.0], |p| {
                [
                    p.normal.x as f32,
                    p.normal.y as f32,
                    p.normal.z as f32,
                    (p.normal.dot(scene.origin) - p.offset) as f32,
                ]
            }));
        }
        queue.write_buffer(&self.uniform, 0, &bytes(values));
        let target = self.target.as_ref().expect("target resized");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("BIM depth pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    // Transparent: the egui gradient behind the viewport shows through.
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &target.depth,
                depth_ops: Some(wgpu::Operations {
                    // Reversed-Z (see Camera::matrix): far is 0.
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.uniform_group, &[]);
        // Opaque faces, then see-through faces over them, then edges.
        let wireframe = display.mode == crate::scene::RenderMode::Wireframe;
        for (layer, pipeline) in [(0, &self.pipeline), (1, &self.transparent)] {
            if wireframe {
                break;
            }
            pass.set_pipeline(pipeline);
            for (i, mesh) in self.meshes.iter().enumerate() {
                if self.counts[i][layer] == 0 {
                    continue;
                }
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_vertex_buffer(1, self.instances[i][layer].slice(..));
                pass.draw(0..mesh.count, 0..self.counts[i][layer]);
            }
        }
        if display.mode != crate::scene::RenderMode::Shaded {
            pass.set_pipeline(&self.edges);
            for layer in 0..2 {
                for (i, mesh) in self.meshes.iter().enumerate() {
                    if self.counts[i][layer] == 0 {
                        continue;
                    }
                    pass.set_vertex_buffer(0, mesh.edges.slice(..));
                    pass.set_vertex_buffer(1, self.instances[i][layer].slice(..));
                    pass.draw(0..mesh.edge_count, 0..self.counts[i][layer]);
                }
            }
        }
    }
}

pub struct ViewportCallback {
    pub scene: Arc<Scene>,
    pub camera: Camera,
    pub state: ViewState,
    pub size: [f32; 2],
}
impl CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if let Some(renderer) = resources.get_mut::<GpuRenderer>() {
            let maximum = device.limits().max_texture_dimension_2d;
            let size = self.size.map(|v| {
                (v * screen.pixels_per_point)
                    .round()
                    .clamp(1.0, maximum as f32) as u32
            });
            renderer.render(device, queue, encoder, self, size);
        }
        Vec::new()
    }
    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        if let Some(renderer) = resources.get::<GpuRenderer>()
            && let Some(target) = &renderer.target
        {
            pass.set_pipeline(&renderer.composite);
            pass.set_bind_group(0, &target.group, &[]);
            pass.draw(0..3, 0..1);
        }
    }
}
