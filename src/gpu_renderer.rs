use std::collections::HashMap;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use image::{Rgba, RgbaImage};
use wgpu::util::DeviceExt;

use crate::model::Map;
use crate::render_assembly::{RenderBillboard, RenderCamera};
use crate::texture::{
    animation_descriptor, AnimationDescriptor, AnimationPlayback, TextureKey, TextureManager,
};

pub const SCENE_WIDTH: u32 = 640;
pub const SCENE_HEIGHT: u32 = 480;
const CHROMA_MOD_FREQ: f32 = 4.0 * std::f32::consts::PI / 15.0;
const NTSC_PHASE_FLIP_HZ: f32 = 29.97002997;

const WALK_PING_PONG: [u32; 4] = [0, 1, 2, 1];
const WALL_HEIGHT: f32 = 1.0;
const CAMERA_HEIGHT: f32 = 0.5;
const NEAR_PLANE: f32 = 0.05;
const FAR_PLANE: f32 = 128.0;
const AFFINE_BLEND: f32 = 0.2;
const SKY_COLOR: &str = "#8489f0"; // Light blue

fn wgpucolor_from_hex_str(hex: &str) -> wgpu::Color {
    let [r, g, b] = [1, 3, 5].map(|i| {
        u8::from_str_radix(&hex[i..i+2], 16).unwrap() as f64 / 255.0
    });
    wgpu::Color { r, g, b, a: 1.0 }
}


#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneVertex {
    position: [f32; 3],
    uv: [f32; 2],
}

impl SceneVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SceneVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    view_proj: [[f32; 4]; 4],
    affine_params: [f32; 4],
    snap_params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct NtscEncodeUniforms {
    source_size: [f32; 2],
    output_size: [f32; 2],
    frame_phase: f32,
    chroma_mod_freq: f32,
    _pad0: [f32; 2],
    mix_row0: [f32; 4],
    mix_row1: [f32; 4],
    mix_row2: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct NtscDecodeUniforms {
    source_size: [f32; 2],
    gamma_exp: f32,
    _pad0: f32,
}

#[derive(Clone, Copy)]
struct AtlasRect {
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    pixel_width: u32,
    pixel_height: u32,
}

#[derive(Clone, Copy)]
enum VisibleSide {
    Front,
    Back,
    Left,
    Right,
}

pub struct SceneRenderer {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    scene_width: u32,
    scene_height: u32,
    scene_view: wgpu::TextureView,
    composite_view: wgpu::TextureView,
    decoded_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    scene_bind_group: wgpu::BindGroup,
    ntsc_encode_bind_group: wgpu::BindGroup,
    ntsc_decode_bind_group: wgpu::BindGroup,
    blit_bind_group: wgpu::BindGroup,
    scene_pipeline: wgpu::RenderPipeline,
    ntsc_encode_pipeline: wgpu::RenderPipeline,
    ntsc_decode_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    scene_uniform_buffer: wgpu::Buffer,
    ntsc_encode_uniform_buffer: wgpu::Buffer,
    ntsc_decode_uniform_buffer: wgpu::Buffer,
    wall_vertex_buffer: wgpu::Buffer,
    wall_index_buffer: wgpu::Buffer,
    wall_index_count: u32,
    sprite_vertex_buffer: wgpu::Buffer,
    sprite_vertex_capacity: usize,
    sprite_vertex_count: u32,
    atlas_rects: Vec<AtlasRect>,
    atlas_index_by_texture: HashMap<TextureKey, usize>,
}

impl SceneRenderer {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        map: &Map,
        texture_manager: &TextureManager,
        surface_format: wgpu::TextureFormat,
        scene_width: u32,
        scene_height: u32,
    ) -> Self {
        let composite_width = scene_width * 4;
        let composite_height = scene_height;
        let decoded_width = scene_width * 2;
        let decoded_height = scene_height;
        let (atlas_image, atlas_rects) = build_texture_atlas(texture_manager.images());
        let atlas_index_by_texture = texture_manager
            .images()
            .iter()
            .enumerate()
            .map(|(index, _)| (texture_manager.key_at_index(index), index))
            .collect();
        let atlas_size = atlas_image.dimensions();
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cpu_game_texture_atlas"),
            size: wgpu::Extent3d {
                width: atlas_size.0,
                height: atlas_size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            atlas_image.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * atlas_size.0),
                rows_per_image: Some(atlas_size.1),
            },
            wgpu::Extent3d {
                width: atlas_size.0,
                height: atlas_size.1,
                depth_or_array_layers: 1,
            },
        );

        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cpu_game_atlas_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let scene_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cpu_game_scene_texture"),
            size: wgpu::Extent3d {
                width: scene_width,
                height: scene_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let scene_view = scene_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let composite_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cpu_game_composite_texture"),
            size: wgpu::Extent3d {
                width: composite_width,
                height: composite_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let composite_view = composite_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let decoded_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cpu_game_decoded_texture"),
            size: wgpu::Extent3d {
                width: decoded_width,
                height: decoded_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let decoded_view = decoded_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let scene_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cpu_game_scene_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let composite_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cpu_game_composite_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cpu_game_scene_depth"),
            size: wgpu::Extent3d {
                width: scene_width,
                height: scene_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let scene_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cpu_game_scene_uniforms"),
            contents: bytemuck::bytes_of(&SceneUniforms {
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                affine_params: [AFFINE_BLEND, scene_width as f32, scene_height as f32, 0.0],
                snap_params: [scene_width as f32 / 2.0, scene_height as f32 / 2.0, 0.0, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let ntsc_encode_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cpu_game_ntsc_encode_uniforms"),
            contents: bytemuck::bytes_of(&build_encode_uniforms(
                scene_width,
                scene_height,
                0.0,
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let ntsc_decode_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cpu_game_ntsc_decode_uniforms"),
            contents: bytemuck::bytes_of(&build_decode_uniforms(composite_width, composite_height)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let scene_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cpu_game_scene_bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cpu_game_scene_bg"),
            layout: &scene_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: scene_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        let blit_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cpu_game_blit_bgl"),
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
        let post_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cpu_game_post_bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let ntsc_encode_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cpu_game_ntsc_encode_bg"),
            layout: &post_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&scene_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: ntsc_encode_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let ntsc_decode_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cpu_game_ntsc_decode_bg"),
            layout: &post_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&composite_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&composite_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: ntsc_decode_uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let blit_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cpu_game_blit_bg"),
            layout: &blit_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&decoded_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&scene_sampler),
                },
            ],
        });

        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cpu_game_scene_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu_renderer_scene.wgsl").into()),
        });
        let ntsc_encode_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cpu_game_ntsc_encode_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu_renderer_ntsc_encode.wgsl").into()),
        });
        let ntsc_decode_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cpu_game_ntsc_decode_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu_renderer_ntsc_decode.wgsl").into()),
        });
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cpu_game_blit_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("gpu_renderer_blit.wgsl").into()),
        });

        let scene_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cpu_game_scene_pl"),
            bind_group_layouts: &[&scene_bind_group_layout],
            push_constant_ranges: &[],
        });
        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cpu_game_scene_pipeline"),
            layout: Some(&scene_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &scene_shader,
                entry_point: Some("vs_main"),
                buffers: &[SceneVertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &scene_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cpu_game_post_pl"),
            bind_group_layouts: &[&post_bind_group_layout],
            push_constant_ranges: &[],
        });
        let ntsc_encode_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cpu_game_ntsc_encode_pipeline"),
            layout: Some(&post_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &ntsc_encode_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &ntsc_encode_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let ntsc_decode_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cpu_game_ntsc_decode_pipeline"),
            layout: Some(&post_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &ntsc_decode_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &ntsc_decode_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cpu_game_blit_pl"),
            bind_group_layouts: &[&blit_bind_group_layout],
            push_constant_ranges: &[],
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cpu_game_blit_pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let (wall_vertices, wall_indices) = build_static_mesh(map, &atlas_rects, texture_manager);
        let wall_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cpu_game_wall_vertices"),
            contents: bytemuck::cast_slice(&wall_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let wall_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cpu_game_wall_indices"),
            contents: bytemuck::cast_slice(&wall_indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let sprite_vertex_capacity = 6;
        let sprite_vertex_buffer = create_sprite_buffer(&device, sprite_vertex_capacity);

        Self {
            device,
            queue,
            scene_width,
            scene_height,
            scene_view,
            composite_view,
            decoded_view,
            depth_view,
            scene_bind_group,
            ntsc_encode_bind_group,
            ntsc_decode_bind_group,
            blit_bind_group,
            scene_pipeline,
            ntsc_encode_pipeline,
            ntsc_decode_pipeline,
            blit_pipeline,
            scene_uniform_buffer,
            ntsc_encode_uniform_buffer,
            ntsc_decode_uniform_buffer,
            wall_vertex_buffer,
            wall_index_buffer,
            wall_index_count: wall_indices.len() as u32,
            sprite_vertex_buffer,
            sprite_vertex_capacity,
            sprite_vertex_count: 0,
            atlas_rects,
            atlas_index_by_texture,
        }
    }

    pub fn render_frame(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        viewport: (u32, u32, u32, u32),
        camera: &RenderCamera,
        billboards: &[RenderBillboard],
        anim_elapsed_ms: f64,
        _frame_number: u64,
    ) {
        let view_proj = build_view_projection(camera, self.scene_width, self.scene_height);
        self.queue.write_buffer(
            &self.scene_uniform_buffer,
            0,
            bytemuck::bytes_of(&SceneUniforms {
                view_proj: view_proj.to_cols_array_2d(),
                affine_params: [AFFINE_BLEND, self.scene_width as f32, self.scene_height as f32, 0.0],
                snap_params: [self.scene_width as f32 / 2.0, self.scene_height as f32 / 2.0, 0.0, 0.0],
            }),
        );
        self.queue.write_buffer(
            &self.ntsc_encode_uniform_buffer,
            0,
            bytemuck::bytes_of(&build_encode_uniforms(
                self.scene_width,
                self.scene_height,
                anim_elapsed_ms,
            )),
        );
        self.queue.write_buffer(
            &self.ntsc_decode_uniform_buffer,
            0,
            bytemuck::bytes_of(&build_decode_uniforms(
                self.scene_width * 4,
                self.scene_height,
            )),
        );

        let sprite_vertices = build_sprite_vertices(
            camera,
            billboards,
            &self.atlas_rects,
            &self.atlas_index_by_texture,
            anim_elapsed_ms,
        );
        self.ensure_sprite_capacity(sprite_vertices.len());
        if !sprite_vertices.is_empty() {
            self.queue.write_buffer(
                &self.sprite_vertex_buffer,
                0,
                bytemuck::cast_slice(&sprite_vertices),
            );
        }
        self.sprite_vertex_count = sprite_vertices.len() as u32;

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cpu_game_scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpucolor_from_hex_str(SKY_COLOR)),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.scene_pipeline);
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.set_vertex_buffer(0, self.wall_vertex_buffer.slice(..));
            pass.set_index_buffer(self.wall_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.wall_index_count, 0, 0..1);

            if self.sprite_vertex_count > 0 {
                pass.set_vertex_buffer(0, self.sprite_vertex_buffer.slice(..));
                pass.draw(0..self.sprite_vertex_count, 0..1);
            }
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cpu_game_ntsc_encode_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.composite_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.ntsc_encode_pipeline);
            pass.set_bind_group(0, &self.ntsc_encode_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cpu_game_ntsc_decode_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.decoded_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.ntsc_decode_pipeline);
            pass.set_bind_group(0, &self.ntsc_decode_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        let (vx, vy, vw, vh) = viewport;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("cpu_game_blit_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(vx as f32, vy as f32, vw as f32, vh as f32, 0.0, 1.0);
        pass.set_pipeline(&self.blit_pipeline);
        pass.set_bind_group(0, &self.blit_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    pub fn calculate_aspect_preserving_viewport(
        window_width: u32,
        window_height: u32,
        content_width: u32,
        content_height: u32,
    ) -> (u32, u32, u32, u32) {
        let window_aspect = window_width as f32 / window_height as f32;
        let content_aspect = content_width as f32 / content_height as f32;

        if window_aspect > content_aspect {
            let scaled_width = window_height as f32 * content_aspect;
            let x_offset = (window_width as f32 - scaled_width) / 2.0;
            (x_offset as u32, 0, scaled_width as u32, window_height)
        } else {
            let scaled_height = window_width as f32 / content_aspect;
            let y_offset = (window_height as f32 - scaled_height) / 2.0;
            (0, y_offset as u32, window_width, scaled_height as u32)
        }
    }

    pub fn calculate_scene_width(window_width: u32, window_height: u32) -> u32 {
        if window_width == 0 || window_height == 0 {
            return SCENE_WIDTH;
        }

        ((window_width as f32 / window_height as f32) * SCENE_HEIGHT as f32)
            .round()
            .max(1.0) as u32
    }

    pub fn scene_size(&self) -> (u32, u32) {
        (self.scene_width, self.scene_height)
    }

    fn ensure_sprite_capacity(&mut self, required_vertices: usize) {
        if required_vertices <= self.sprite_vertex_capacity {
            return;
        }

        self.sprite_vertex_capacity = required_vertices.next_power_of_two().max(6);
        self.sprite_vertex_buffer = create_sprite_buffer(&self.device, self.sprite_vertex_capacity);
    }
}

fn create_sprite_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cpu_game_sprite_vertices"),
        size: (vertex_capacity * std::mem::size_of::<SceneVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn build_view_projection(camera: &RenderCamera, scene_width: u32, scene_height: u32) -> Mat4 {
    let aspect = scene_width as f32 / scene_height as f32;
    let plane_len = ((camera.plane_x * camera.plane_x) + (camera.plane_y * camera.plane_y)).sqrt() as f32;
    // plane_len is tan(half_hfov), convert to vfov for perspective_lh
    let vfov = 2.0 * (plane_len / aspect).atan();

    let eye = Vec3::new(camera.x as f32, CAMERA_HEIGHT, camera.y as f32);
    let forward = Vec3::new(camera.dir_x as f32, 0.0, camera.dir_y as f32);
    let view = Mat4::look_to_lh(eye, forward, Vec3::Y);
    let projection = Mat4::perspective_lh(vfov, aspect, NEAR_PLANE, FAR_PLANE);
    projection * view
}

fn build_encode_uniforms(scene_width: u32, scene_height: u32, anim_elapsed_ms: f64) -> NtscEncodeUniforms {
    let elapsed_seconds = (anim_elapsed_ms * 0.001) as f32;
    let frame_phase = (elapsed_seconds * NTSC_PHASE_FLIP_HZ).floor().rem_euclid(2.0);

    NtscEncodeUniforms {
        source_size: [scene_width as f32, scene_height as f32],
        output_size: [(scene_width * 4) as f32, scene_height as f32],
        frame_phase,
        chroma_mod_freq: CHROMA_MOD_FREQ,
        _pad0: [0.0; 2],
        mix_row0: [1.0, 1.0, 1.0, 0.0],
        mix_row1: [1.0, 2.0, 0.0, 0.0],
        mix_row2: [1.0, 0.0, 2.0, 0.0],
    }
}

fn build_decode_uniforms(composite_width: u32, composite_height: u32) -> NtscDecodeUniforms {
    NtscDecodeUniforms {
        source_size: [composite_width as f32, composite_height as f32],
        gamma_exp: 2.5 / 2.0,
        _pad0: 0.0,
    }
}

fn build_texture_atlas(textures: &[RgbaImage]) -> (RgbaImage, Vec<AtlasRect>) {
    let padding = 1u32;
    let width = textures
        .iter()
        .map(|texture| texture.width() + padding)
        .sum::<u32>()
        + padding;
    let height = textures.iter().map(image::GenericImageView::height).max().unwrap_or(1) + padding * 2;

    let mut atlas = RgbaImage::from_pixel(width.max(1), height.max(1), Rgba([0, 0, 0, 0]));
    let mut rects = Vec::with_capacity(textures.len());
    let mut cursor_x = padding;

    for texture in textures {
        for y in 0..texture.height() {
            for x in 0..texture.width() {
                let pixel = texture.get_pixel(x, y);
                atlas.put_pixel(cursor_x + x, padding + y, *pixel);
            }
        }

        rects.push(AtlasRect {
            u0: cursor_x as f32 / width as f32,
            v0: padding as f32 / height as f32,
            u1: (cursor_x + texture.width()) as f32 / width as f32,
            v1: (padding + texture.height()) as f32 / height as f32,
            pixel_width: texture.width(),
            pixel_height: texture.height(),
        });
        cursor_x += texture.width() + padding;
    }

    (atlas, rects)
}

fn build_static_mesh(
    map: &Map,
    atlas_rects: &[AtlasRect],
    texture_manager: &TextureManager,
) -> (Vec<SceneVertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let height = map.tiles.len();
    let width = map.tiles.first().map_or(0, Vec::len);

    for z in 0..height {
        for x in 0..width {
            if !map.is_wall(x, z) {
                let texture_index = texture_manager.texture_index(TextureKey::Floor(map.floor_at(x, z)));
                let rect = inset_atlas_rect_half_texel(
                    atlas_rects[texture_index.min(atlas_rects.len().saturating_sub(1))],
                );
                let x0 = x as f32;
                let x1 = x0 + 1.0;
                let z0 = z as f32;
                let z1 = z0 + 1.0;

                push_quad(
                    &mut vertices,
                    &mut indices,
                    rect,
                    false,
                    [x0, 0.0, z1],
                    [x1, 0.0, z1],
                    [x1, 0.0, z0],
                    [x0, 0.0, z0],
                );
                continue;
            }

            let texture_key = texture_manager.wall_texture(map.tile_at(x, z));
            let texture_index = texture_manager.texture_index(texture_key);
            let rect = inset_atlas_rect_half_texel(
                atlas_rects[texture_index.min(atlas_rects.len().saturating_sub(1))],
            );
            let x0 = x as f32;
            let x1 = x0 + 1.0;
            let z0 = z as f32;
            let z1 = z0 + 1.0;

            let left_empty = x == 0 || !map.is_wall(x - 1, z);
            let right_empty = x + 1 >= width || !map.is_wall(x + 1, z);
            let north_empty = z == 0 || !map.is_wall(x, z - 1);
            let south_empty = z + 1 >= height || !map.is_wall(x, z + 1);

            if left_empty {
                push_quad(
                    &mut vertices,
                    &mut indices,
                    rect,
                    false,
                    [x0, 0.0, z1],
                    [x0, 0.0, z0],
                    [x0, WALL_HEIGHT, z0],
                    [x0, WALL_HEIGHT, z1],
                );
            }
            if right_empty {
                push_quad(
                    &mut vertices,
                    &mut indices,
                    rect,
                    false,
                    [x1, 0.0, z0],
                    [x1, 0.0, z1],
                    [x1, WALL_HEIGHT, z1],
                    [x1, WALL_HEIGHT, z0],
                );
            }
            if north_empty {
                push_quad(
                    &mut vertices,
                    &mut indices,
                    rect,
                    true,
                    [x1, 0.0, z0],
                    [x0, 0.0, z0],
                    [x0, WALL_HEIGHT, z0],
                    [x1, WALL_HEIGHT, z0],
                );
            }
            if south_empty {
                push_quad(
                    &mut vertices,
                    &mut indices,
                    rect,
                    true,
                    [x0, 0.0, z1],
                    [x1, 0.0, z1],
                    [x1, WALL_HEIGHT, z1],
                    [x0, WALL_HEIGHT, z1],
                );
            }
        }
    }

    (vertices, indices)
}

fn inset_atlas_rect_half_texel(rect: AtlasRect) -> AtlasRect {
    let width_scale = (rect.u1 - rect.u0) / rect.pixel_width.max(1) as f32;
    let height_scale = (rect.v1 - rect.v0) / rect.pixel_height.max(1) as f32;
    let half_texel_u = width_scale * 0.5;
    let half_texel_v = height_scale * 0.5;

    AtlasRect {
        u0: rect.u0 + half_texel_u,
        v0: rect.v0 + half_texel_v,
        u1: rect.u1 - half_texel_u,
        v1: rect.v1 - half_texel_v,
        pixel_width: rect.pixel_width,
        pixel_height: rect.pixel_height,
    }
}

fn build_sprite_vertices(
    camera: &RenderCamera,
    billboards: &[RenderBillboard],
    atlas_rects: &[AtlasRect],
    atlas_index_by_texture: &HashMap<TextureKey, usize>,
    anim_elapsed_ms: f64,
) -> Vec<SceneVertex> {
    let mut sorted = billboards.to_vec();
    sorted.sort_by(|left, right| {
        let left_dist = (left.x - camera.x).powi(2) + (left.y - camera.y).powi(2);
        let right_dist = (right.x - camera.x).powi(2) + (right.y - camera.y).powi(2);
        right_dist.total_cmp(&left_dist)
    });

    let right = Vec3::new(camera.plane_x as f32, 0.0, camera.plane_y as f32).normalize_or_zero();
    let mut vertices = Vec::with_capacity(sorted.len() * 6);
    let camera_facing_angle = camera.dir_y.atan2(camera.dir_x);

    for billboard in sorted {
        let Some(texture_index) = atlas_index_by_texture.get(&billboard.texture).copied() else {
            continue;
        };
        let Some(rect) = atlas_rects.get(texture_index) else {
            continue;
        };
        let uv_rect =
            select_sprite_uv_rect(*rect, &billboard, camera_facing_angle, anim_elapsed_ms);
        let center = Vec3::new(billboard.x as f32, 0.0, billboard.y as f32);
        let half_width = right * (billboard.width * 0.5);
        let up = Vec3::new(0.0, billboard.height, 0.0);
        let bottom_left = center - half_width;
        let bottom_right = center + half_width;
        let top_left = bottom_left + up;
        let top_right = bottom_right + up;

        vertices.extend_from_slice(&[
            SceneVertex {
                position: bottom_left.to_array(),
                uv: [uv_rect.u0, uv_rect.v1],
            },
            SceneVertex {
                position: bottom_right.to_array(),
                uv: [uv_rect.u1, uv_rect.v1],
            },
            SceneVertex {
                position: top_right.to_array(),
                uv: [uv_rect.u1, uv_rect.v0],
            },
            SceneVertex {
                position: bottom_left.to_array(),
                uv: [uv_rect.u0, uv_rect.v1],
            },
            SceneVertex {
                position: top_right.to_array(),
                uv: [uv_rect.u1, uv_rect.v0],
            },
            SceneVertex {
                position: top_left.to_array(),
                uv: [uv_rect.u0, uv_rect.v0],
            },
        ]);
    }

    vertices
}

fn select_sprite_uv_rect(
    rect: AtlasRect,
    billboard: &RenderBillboard,
    camera_facing_angle: f64,
    anim_elapsed_ms: f64,
) -> AtlasRect {
    let Some(animation) = animation_descriptor(billboard.animation) else {
        return rect;
    };
    let atlas_width = animation.columns * animation.frame_width;
    let atlas_height = animation.rows * animation.frame_height;
    if (rect.pixel_width as usize) < atlas_width || (rect.pixel_height as usize) < atlas_height {
        return rect;
    }

    let frame_step = (anim_elapsed_ms / animation.ms_per_frame).floor() as u32;
    let frame_col = animation_frame_column(animation, frame_step, billboard.is_moving);
    let side_row = if animation.directional_rows && matches!(billboard.facing_mode, crate::texture::FacingMode::Movement) {
        side_to_row(get_visible_side(
            billboard.movement_angle,
            camera_facing_angle,
        )) as usize
    } else {
        animation_frame_row(animation, frame_step)
    };

    let frame_origin_x = frame_col * animation.frame_width;
    let frame_origin_y = side_row * animation.frame_height;
    let width_scale = (rect.u1 - rect.u0) / rect.pixel_width as f32;
    let height_scale = (rect.v1 - rect.v0) / rect.pixel_height as f32;
    let half_texel_u = width_scale * 0.5;
    let half_texel_v = height_scale * 0.5;

    AtlasRect {
        u0: rect.u0 + frame_origin_x as f32 * width_scale + half_texel_u,
        v0: rect.v0 + frame_origin_y as f32 * height_scale + half_texel_v,
        u1: rect.u0 + (frame_origin_x + animation.frame_width) as f32 * width_scale - half_texel_u,
        v1: rect.v0 + (frame_origin_y + animation.frame_height) as f32 * height_scale - half_texel_v,
        pixel_width: animation.frame_width as u32,
        pixel_height: animation.frame_height as u32,
    }
}

fn animation_frame_column(
    animation: AnimationDescriptor,
    frame_step: u32,
    is_moving: bool,
) -> usize {
    match animation.playback {
        AnimationPlayback::PingPong => walk_frame_col(frame_step, is_moving) as usize,
        AnimationPlayback::Loop => (frame_step as usize) % animation.columns.max(1),
    }
}

fn animation_frame_row(animation: AnimationDescriptor, frame_step: u32) -> usize {
    if animation.directional_rows {
        0
    } else {
        match animation.playback {
            AnimationPlayback::PingPong => 0,
            AnimationPlayback::Loop => (frame_step as usize) % animation.rows.max(1),
        }
    }
}

fn get_visible_side(entity_movement_angle: f64, camera_facing_angle: f64) -> VisibleSide {
    const SIDE_HALF_ANGLE: f64 = 0.785398;

    let mut rel = entity_movement_angle - camera_facing_angle;
    rel = (rel + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;

    let abs_rel = rel.abs();

    if abs_rel < SIDE_HALF_ANGLE {
        VisibleSide::Back
    } else if abs_rel > std::f64::consts::PI - SIDE_HALF_ANGLE {
        VisibleSide::Front
    } else if abs_rel > std::f64::consts::FRAC_PI_2 - SIDE_HALF_ANGLE
        && abs_rel < std::f64::consts::FRAC_PI_2 + SIDE_HALF_ANGLE
    {
        if rel > 0.0 {
            VisibleSide::Left
        } else {
            VisibleSide::Right
        }
    } else if abs_rel < std::f64::consts::FRAC_PI_2 {
        VisibleSide::Back
    } else {
        VisibleSide::Front
    }
}

fn side_to_row(side: VisibleSide) -> u32 {
    match side {
        VisibleSide::Front => 0,
        VisibleSide::Back => 1,
        VisibleSide::Left => 2,
        VisibleSide::Right => 3,
    }
}

fn walk_frame_col(frame_step: u32, is_moving: bool) -> u32 {
    if !is_moving {
        1
    } else {
        WALK_PING_PONG[(frame_step % WALK_PING_PONG.len() as u32) as usize]
    }
}

fn push_quad(
    vertices: &mut Vec<SceneVertex>,
    indices: &mut Vec<u32>,
    rect: AtlasRect,
    flip_u: bool,
    p0: [f32; 3],
    p1: [f32; 3],
    p2: [f32; 3],
    p3: [f32; 3],
) {
    // Subdivide each quad into a 2x2 grid (4 sub-quads = 8 triangles) via bilinear interpolation.
    // Corner parameterization: p0=(s=0,t=0), p1=(s=1,t=0), p2=(s=1,t=1), p3=(s=0,t=1)
    const DIVS: usize = 2;

    let (left_u, right_u) = if flip_u {
        (rect.u1, rect.u0)
    } else {
        (rect.u0, rect.u1)
    };

    let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]
    };

    let point_at = |s: f32, t: f32| -> SceneVertex {
        let bottom = lerp3(p0, p1, s);
        let top = lerp3(p3, p2, s);
        let pos = lerp3(bottom, top, t);
        let u = left_u + (right_u - left_u) * s;
        let v = rect.v1 + (rect.v0 - rect.v1) * t;
        SceneVertex { position: pos, uv: [u, v] }
    };

    for ti in 0..DIVS {
        for si in 0..DIVS {
            let s0 = si as f32 / DIVS as f32;
            let s1 = (si + 1) as f32 / DIVS as f32;
            let t0 = ti as f32 / DIVS as f32;
            let t1 = (ti + 1) as f32 / DIVS as f32;
            let base = vertices.len() as u32;
            vertices.extend_from_slice(&[
                point_at(s0, t0),
                point_at(s1, t0),
                point_at(s1, t1),
                point_at(s0, t1),
            ]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
}