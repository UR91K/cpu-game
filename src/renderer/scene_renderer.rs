use std::collections::HashMap;
use std::sync::Arc;

use crate::model::Level;
use crate::render_assembly::{RenderBillboard, RenderCamera};
use crate::renderer::atlas::build_texture_atlas;
use crate::renderer::mesh::{
    AtlasRect, build_sprite_vertices, build_static_mesh, create_sprite_buffer,
};
use crate::renderer::uniforms::{
    SceneUniforms, SceneVertex, SkyUniforms, build_decode_uniforms, build_encode_uniforms,
};
use crate::texture::{TextureKey, TextureManager};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

pub const SCENE_WIDTH: u32 = 640;
pub const SCENE_HEIGHT: u32 = 480;
pub const NEAR_PLANE: f32 = 0.05;
pub const FAR_PLANE: f32 = 128.0;
pub const CAMERA_HEIGHT: f32 = 0.6;
pub const AFFINE_BLEND: f32 = 0.4;
const SKY_PITCH_RADIANS: f32 = 0.15;

fn build_view_projection(camera: &RenderCamera, scene_width: u32, scene_height: u32) -> Mat4 {
    let aspect = scene_width as f32 / scene_height as f32;
    let plane_len =
        ((camera.plane_x * camera.plane_x) + (camera.plane_y * camera.plane_y)).sqrt() as f32;
    // plane_len is tan(half_hfov), convert to vfov for perspective_lh
    let vfov = 2.0 * (plane_len / aspect).atan();

    let eye = Vec3::new(camera.x as f32, CAMERA_HEIGHT, camera.y as f32);
    let forward = Vec3::new(camera.dir_x as f32, 0.0, camera.dir_y as f32);
    let view = Mat4::look_to_lh(eye, forward, Vec3::Y);
    let projection = Mat4::perspective_lh(vfov, aspect, NEAR_PLANE, FAR_PLANE);
    projection * view
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
    overlay_bind_group: wgpu::BindGroup,
    sky_bind_group: wgpu::BindGroup,
    scene_pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    ntsc_encode_pipeline: wgpu::RenderPipeline,
    ntsc_decode_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    overlay_texture: wgpu::Texture,
    scene_uniform_buffer: wgpu::Buffer,
    sky_uniform_buffer: wgpu::Buffer,
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
        level: &Level,
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

        let overlay_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cpu_game_overlay"),
            size: wgpu::Extent3d {
                width: scene_width,
                height: scene_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let overlay_view = overlay_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let overlay_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cpu_game_overlay_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let scene_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cpu_game_scene_uniforms"),
            contents: bytemuck::bytes_of(&SceneUniforms {
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                affine_params: [AFFINE_BLEND, scene_width as f32, scene_height as f32, 0.0],
                snap_params: [ // divide these by 2.0 for half res PSX mode
                    scene_width as f32,
                    scene_height as f32,
                    0.0,
                    0.0,
                ],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sky_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cpu_game_sky_uniforms"),
            contents: bytemuck::bytes_of(&SkyUniforms {
                time_resolution: [0.0, scene_width as f32, scene_height as f32, SKY_PITCH_RADIANS],
                camera_origin: [0.0, CAMERA_HEIGHT, 0.0, 1.0],
                camera_forward: [0.0, 0.0, 1.0, 0.0],
                camera_right: [1.0, 0.0, 0.0, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let ntsc_encode_uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("cpu_game_ntsc_encode_uniforms"),
                contents: bytemuck::bytes_of(&build_encode_uniforms(
                    scene_width,
                    scene_height,
                    0.0,
                )),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let ntsc_decode_uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("cpu_game_ntsc_decode_uniforms"),
                contents: bytemuck::bytes_of(&build_decode_uniforms(
                    composite_width,
                    composite_height,
                )),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let scene_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let sky_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cpu_game_sky_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let sky_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cpu_game_sky_bg"),
            layout: &sky_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: sky_uniform_buffer.as_entire_binding(),
            }],
        });

        let blit_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let post_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let overlay_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cpu_game_overlay_bg"),
            layout: &blit_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&overlay_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&overlay_sampler),
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
        let sky_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cpu_game_sky_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sky.wgsl").into()),
        });

        let sky_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cpu_game_sky_pl"),
            bind_group_layouts: &[&sky_bind_group_layout],
            push_constant_ranges: &[],
        });
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cpu_game_sky_pipeline"),
            layout: Some(&sky_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
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

        let scene_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cpu_game_overlay_pipeline"),
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
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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

        let (wall_vertices, wall_indices) = build_static_mesh(level, &atlas_rects, texture_manager);
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
            overlay_bind_group,
            sky_bind_group,
            scene_pipeline,
            sky_pipeline,
            ntsc_encode_pipeline,
            ntsc_decode_pipeline,
            blit_pipeline,
            overlay_pipeline,
            overlay_texture,
            scene_uniform_buffer,
            sky_uniform_buffer,
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
        overlay: Option<&[u8]>,
    ) {
        let view_proj = build_view_projection(camera, self.scene_width, self.scene_height);
        let plane_len =
            ((camera.plane_x * camera.plane_x) + (camera.plane_y * camera.plane_y)).sqrt() as f32;
        let right = Vec3::new(camera.plane_x as f32, 0.0, camera.plane_y as f32)
            .normalize_or_zero();
        let right = if right.length_squared() > 0.0 {
            right
        } else {
            Vec3::new(camera.dir_y as f32, 0.0, -camera.dir_x as f32)
        };
        self.queue.write_buffer(
            &self.scene_uniform_buffer,
            0,
            bytemuck::bytes_of(&SceneUniforms {
                view_proj: view_proj.to_cols_array_2d(),
                affine_params: [
                    AFFINE_BLEND,
                    self.scene_width as f32,
                    self.scene_height as f32,
                    0.0,
                ],
                snap_params: [ // divide these by 2.0 to match PSX 'high resolution mode' exactly. keep it at full res for low res mode.
                    self.scene_width as f32, 
                    self.scene_height as f32,
                    0.0,
                    0.0,
                ],
            }),
        );
        self.queue.write_buffer(
            &self.sky_uniform_buffer,
            0,
            bytemuck::bytes_of(&SkyUniforms {
                time_resolution: [
                    (anim_elapsed_ms * 0.001) as f32,
                    self.scene_width as f32,
                    self.scene_height as f32,
                    SKY_PITCH_RADIANS,
                ],
                camera_origin: [camera.x as f32, CAMERA_HEIGHT, camera.y as f32, plane_len],
                camera_forward: [camera.dir_x as f32, 0.0, camera.dir_y as f32, 0.0],
                camera_right: [right.x, right.y, right.z, 0.0],
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
                label: Some("cpu_game_sky_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
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
            pass.set_pipeline(&self.sky_pipeline);
            pass.set_bind_group(0, &self.sky_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cpu_game_scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
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

        if let Some(pixels) = overlay {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.overlay_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * self.scene_width),
                    rows_per_image: Some(self.scene_height),
                },
                wgpu::Extent3d {
                    width: self.scene_width,
                    height: self.scene_height,
                    depth_or_array_layers: 1,
                },
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cpu_game_overlay_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.overlay_pipeline);
            pass.set_bind_group(0, &self.overlay_bind_group, &[]);
            pass.draw(0..3, 0..1);
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
        {
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
    }

    pub fn render_overlay_only(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        output_view: &wgpu::TextureView,
        viewport: (u32, u32, u32, u32),
        overlay: Option<&[u8]>,
    ) {
        if let Some(pixels) = overlay {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.overlay_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * self.scene_width),
                    rows_per_image: Some(self.scene_height),
                },
                wgpu::Extent3d {
                    width: self.scene_width,
                    height: self.scene_height,
                    depth_or_array_layers: 1,
                },
            );
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cpu_game_overlay_only_scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
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
            pass.set_pipeline(&self.overlay_pipeline);
            pass.set_bind_group(0, &self.overlay_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cpu_game_overlay_only_encode_pass"),
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
                label: Some("cpu_game_overlay_only_decode_pass"),
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
            label: Some("cpu_game_overlay_only_pass"),
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
