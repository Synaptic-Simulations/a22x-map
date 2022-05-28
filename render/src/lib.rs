use std::path::PathBuf;

use geo::LoadError;
use wgpu::{
	include_spirv,
	AddressMode,
	BindGroup,
	BindGroupDescriptor,
	BindGroupEntry,
	BindGroupLayout,
	BindGroupLayoutDescriptor,
	BindGroupLayoutEntry,
	BindingResource,
	BindingType,
	Buffer,
	BufferBindingType,
	BufferDescriptor,
	BufferUsages,
	Color,
	ColorTargetState,
	CommandEncoder,
	Device,
	Extent3d,
	FilterMode,
	FragmentState,
	LoadOp,
	Operations,
	PipelineLayoutDescriptor,
	PrimitiveState,
	PrimitiveTopology,
	Queue,
	RenderPassColorAttachment,
	RenderPassDescriptor,
	RenderPipeline,
	RenderPipelineDescriptor,
	SamplerBindingType,
	SamplerDescriptor,
	ShaderStages,
	TextureDescriptor,
	TextureDimension,
	TextureFormat,
	TextureSampleType,
	TextureUsages,
	TextureView,
	TextureViewDimension,
	VertexState,
};

use crate::{range::Range, tile_cache::TileCache};

pub mod range;
mod tile_cache;

/// A polar coordinate, in degrees.
#[derive(Copy, Clone, Debug)]
pub struct LatLon {
	pub lat: f32,
	pub lon: f32,
}

pub struct RendererOptions {
	pub data_path: PathBuf,
	pub width: u32,
	pub height: u32,
	pub output_format: TextureFormat,
}

pub struct FrameOptions {
	pub position: LatLon,
	pub range: Range,
	pub heading: f32,
	pub sun_azimuth: f32,
	pub sun_elevation: f32,
	pub altitude: f32,
}

impl Default for FrameOptions {
	fn default() -> Self {
		FrameOptions {
			position: LatLon { lat: 0.0, lon: 0.0 },
			range: Range::Nm40,
			heading: 0.,
			sun_azimuth: 315.,
			sun_elevation: 45.,
			altitude: 10000.,
		}
	}
}

pub struct Renderer {
	cache: TileCache,
	cbuffer: Buffer,
	aspect_ratio: f32,
	heightmap: TextureView,
	heightmap_layout: BindGroupLayout,
	heightmap_pipeline: RenderPipeline,
	heightmap_group: BindGroup,
	final_pipeline: RenderPipeline,
	final_group: BindGroup,
}

impl Renderer {
	const HEIGHTMAP_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

	pub fn new(device: &Device, options: &RendererOptions) -> Result<Self, LoadError> {
		let aspect_ratio = options.width as f32 / options.height as f32;

		let sets = std::fs::read_to_string(options.data_path.join("_meta"))?;
		let datasets = sets.lines().map(|line| options.data_path.join(line)).collect();
		let cache = TileCache::new(device, aspect_ratio, datasets)?;

		let cbuffer = device.create_buffer(&BufferDescriptor {
			label: Some("Map Render Constant Buffer"),
			size: 48,
			usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		let vertex = device.create_shader_module(&include_spirv!(env!("FullscreenVS.hlsl")));
		let heightmap_fragment = device.create_shader_module(&include_spirv!(env!("HeightmapPS.hlsl")));
		let final_fragment = device.create_shader_module(&include_spirv!(env!("FinalPS.hlsl")));

		let heightmap = Self::make_heightmap(device, options.width, options.height);

		let heightmap_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
			label: Some("Heightmap Bind Group"),
			entries: &[
				BindGroupLayoutEntry {
					binding: 0,
					visibility: ShaderStages::FRAGMENT,
					ty: BindingType::Buffer {
						ty: BufferBindingType::Uniform,
						has_dynamic_offset: false,
						min_binding_size: None,
					},
					count: None,
				},
				BindGroupLayoutEntry {
					binding: 1,
					visibility: ShaderStages::FRAGMENT,
					ty: BindingType::Texture {
						sample_type: TextureSampleType::Uint,
						view_dimension: TextureViewDimension::D2,
						multisampled: false,
					},
					count: None,
				},
				BindGroupLayoutEntry {
					binding: 2,
					visibility: ShaderStages::FRAGMENT,
					ty: BindingType::Buffer {
						ty: BufferBindingType::Storage { read_only: false },
						has_dynamic_offset: false,
						min_binding_size: None,
					},
					count: None,
				},
				BindGroupLayoutEntry {
					binding: 3,
					visibility: ShaderStages::FRAGMENT,
					ty: BindingType::Texture {
						sample_type: TextureSampleType::Sint,
						view_dimension: TextureViewDimension::D2,
						multisampled: false,
					},
					count: None,
				},
			],
		});

		let heightmap_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
			label: Some("Heightmap Pipeline"),
			layout: Some(&device.create_pipeline_layout(&PipelineLayoutDescriptor {
				label: Some("Heightmap Pipeline Layout"),
				bind_group_layouts: &[&heightmap_layout],
				push_constant_ranges: &[],
			})),
			vertex: VertexState {
				module: &vertex,
				entry_point: "Main",
				buffers: &[],
			},
			primitive: PrimitiveState {
				topology: PrimitiveTopology::TriangleList,
				..Default::default()
			},
			depth_stencil: None,
			multisample: Default::default(),
			fragment: Some(FragmentState {
				module: &heightmap_fragment,
				entry_point: "Main",
				targets: &[ColorTargetState::from(Self::HEIGHTMAP_FORMAT)],
			}),
			multiview: None,
		});

		let heightmap_group = Self::make_heightmap_bind_group(device, &heightmap_layout, &cbuffer, &cache);

		let final_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
			label: Some("Final Bind Group"),
			entries: &[
				BindGroupLayoutEntry {
					binding: 0,
					visibility: ShaderStages::FRAGMENT,
					ty: BindingType::Buffer {
						ty: BufferBindingType::Uniform,
						has_dynamic_offset: false,
						min_binding_size: None,
					},
					count: None,
				},
				BindGroupLayoutEntry {
					binding: 1,
					visibility: ShaderStages::FRAGMENT,
					ty: BindingType::Sampler(SamplerBindingType::Filtering),
					count: None,
				},
				BindGroupLayoutEntry {
					binding: 2,
					visibility: ShaderStages::FRAGMENT,
					ty: BindingType::Texture {
						sample_type: TextureSampleType::Float { filterable: true },
						view_dimension: TextureViewDimension::D2,
						multisampled: false,
					},
					count: None,
				},
			],
		});

		let final_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
			label: Some("Final Pipeline"),
			layout: Some(&device.create_pipeline_layout(&PipelineLayoutDescriptor {
				label: Some("Final Pipeline Layout"),
				bind_group_layouts: &[&final_layout],
				push_constant_ranges: &[],
			})),
			vertex: VertexState {
				module: &vertex,
				entry_point: "Main",
				buffers: &[],
			},
			primitive: PrimitiveState {
				topology: PrimitiveTopology::TriangleList,
				..Default::default()
			},
			depth_stencil: None,
			multisample: Default::default(),
			fragment: Some(FragmentState {
				module: &final_fragment,
				entry_point: "Main",
				targets: &[ColorTargetState::from(options.output_format)],
			}),
			multiview: None,
		});

		let final_group = device.create_bind_group(&BindGroupDescriptor {
			label: Some("Final Bind Group"),
			layout: &final_layout,
			entries: &[
				BindGroupEntry {
					binding: 0,
					resource: cbuffer.as_entire_binding(),
				},
				BindGroupEntry {
					binding: 1,
					resource: BindingResource::Sampler(&device.create_sampler(&SamplerDescriptor {
						label: Some("Final Sampler"),
						address_mode_u: AddressMode::ClampToEdge,
						address_mode_v: AddressMode::ClampToEdge,
						address_mode_w: AddressMode::ClampToEdge,
						mag_filter: FilterMode::Nearest,
						min_filter: FilterMode::Nearest,
						mipmap_filter: FilterMode::Nearest,
						lod_min_clamp: 0.,
						lod_max_clamp: 0.,
						compare: None,
						anisotropy_clamp: None,
						border_color: None,
					})),
				},
				BindGroupEntry {
					binding: 2,
					resource: BindingResource::TextureView(&heightmap),
				},
			],
		});

		Ok(Self {
			cache,
			cbuffer,
			aspect_ratio,
			heightmap,
			heightmap_pipeline,
			heightmap_group,
			heightmap_layout,
			final_pipeline,
			final_group,
		})
	}

	pub fn render(
		&mut self, options: &FrameOptions, device: &Device, queue: &Queue, view: &TextureView,
		encoder: &mut CommandEncoder,
	) {
		if self.cache.populate_tiles(device, queue, options.range) {
			self.heightmap_group =
				Self::make_heightmap_bind_group(device, &self.heightmap_layout, &self.cbuffer, &self.cache);
		}

		encoder.clear_buffer(self.cache.tile_status(), 0, None);
		queue.write_buffer(
			&self.cbuffer,
			0,
			&Self::get_cbuffer_data(&self.cache, self.aspect_ratio, options),
		);

		{
			let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
				label: Some("Heightmap Pass"),
				color_attachments: &[RenderPassColorAttachment {
					view: &self.heightmap,
					resolve_target: None,
					ops: Operations {
						load: LoadOp::Clear(Color::BLACK),
						store: true,
					},
				}],
				depth_stencil_attachment: None,
			});
			pass.set_pipeline(&self.heightmap_pipeline);
			pass.set_bind_group(0, &self.heightmap_group, &[]);
			pass.draw(0..3, 0..1);
		}

		let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
			label: Some("Final Pass"),
			color_attachments: &[RenderPassColorAttachment {
				view,
				resolve_target: None,
				ops: Operations {
					load: LoadOp::Clear(Color::BLACK),
					store: true,
				},
			}],
			depth_stencil_attachment: None,
		});
		pass.set_pipeline(&self.final_pipeline);
		pass.set_bind_group(0, &self.final_group, &[]);
		pass.draw(0..3, 0..1);
	}

	pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
		self.aspect_ratio = width as f32 / height as f32;
		self.heightmap = Self::make_heightmap(device, width, height);
	}

	fn make_heightmap(device: &Device, width: u32, height: u32) -> TextureView {
		let tex = device.create_texture(&TextureDescriptor {
			label: Some("Heightmap"),
			size: Extent3d {
				width,
				height,
				depth_or_array_layers: 1,
			},
			mip_level_count: 1,
			sample_count: 1,
			dimension: TextureDimension::D2,
			format: Self::HEIGHTMAP_FORMAT,
			usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
		});
		tex.create_view(&Default::default())
	}

	fn make_heightmap_bind_group(
		device: &Device, layout: &BindGroupLayout, cbuffer: &Buffer, cache: &TileCache,
	) -> BindGroup {
		device.create_bind_group(&BindGroupDescriptor {
			label: Some("Heightmap Bind Group"),
			layout,
			entries: &[
				BindGroupEntry {
					binding: 0,
					resource: cbuffer.as_entire_binding(),
				},
				BindGroupEntry {
					binding: 1,
					resource: BindingResource::TextureView(cache.tile_map()),
				},
				BindGroupEntry {
					binding: 2,
					resource: cache.tile_status().as_entire_binding(),
				},
				BindGroupEntry {
					binding: 3,
					resource: BindingResource::TextureView(&cache.atlas()),
				},
			],
		})
	}

	fn get_cbuffer_data(cache: &TileCache, aspect_ratio: f32, options: &FrameOptions) -> [u8; 48] {
		let mut data = [0; 48];

		data[0..4].copy_from_slice(&options.position.lat.to_radians().to_le_bytes());
		data[4..8].copy_from_slice(&options.position.lon.to_radians().to_le_bytes());

		data[16..20].copy_from_slice(&options.range.vertical_radians().to_le_bytes());
		data[20..24].copy_from_slice(&aspect_ratio.to_le_bytes());
		data[24..28].copy_from_slice(&cache.tile_size_for_range(options.range).to_le_bytes());
		data[28..32].copy_from_slice(&(360. - options.heading).to_radians().to_le_bytes());
		data[32..36].copy_from_slice(&(90. - options.sun_elevation).to_radians().to_le_bytes());
		let mut azimuth = 360. - options.sun_azimuth + 90.;
		if azimuth >= 360. {
			azimuth -= 360.;
		}
		data[36..40].copy_from_slice(&azimuth.to_radians().to_le_bytes());
		data[40..44].copy_from_slice(&options.altitude.to_le_bytes());

		data
	}
}
