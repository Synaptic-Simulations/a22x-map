use std::path::PathBuf;

use geo::LoadError;
use tracy::wgpu::EncoderProfiler;
use wgpu::{
	include_wgsl,
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
	Device,
	FragmentState,
	LoadOp,
	Operations,
	PipelineLayoutDescriptor,
	Queue,
	RenderPassColorAttachment,
	RenderPassDescriptor,
	RenderPipeline,
	RenderPipelineDescriptor,
	ShaderStages,
	TextureFormat,
	TextureSampleType,
	TextureView,
	TextureViewDimension,
	VertexState,
};

use crate::{
	range::Range,
	tile_cache::{TileCache, UploadStatus},
};

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
	layout: BindGroupLayout,
	pipeline: RenderPipeline,
	group: BindGroup,
}

impl Renderer {
	const CBUFFER_SIZE: u64 = 48;

	pub fn new(device: &Device, options: &RendererOptions) -> Result<Self, LoadError> {
		let aspect_ratio = options.width as f32 / options.height as f32;

		let sets = std::fs::read_to_string(options.data_path.join("_meta"))?;
		let datasets = sets.lines().map(|line| options.data_path.join(line)).collect();
		let cache = TileCache::new(device, aspect_ratio, options.height as _, datasets)?;

		let cbuffer = device.create_buffer(&BufferDescriptor {
			label: Some("Map Render Constant Buffer"),
			size: Self::CBUFFER_SIZE,
			usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
			label: Some("Map Render Bind Group"),
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
				BindGroupLayoutEntry {
					binding: 4,
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

		let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
			label: Some("Map Render Pipeline"),
			layout: Some(&device.create_pipeline_layout(&PipelineLayoutDescriptor {
				label: Some("Map Render Pipeline Layout"),
				bind_group_layouts: &[&layout],
				push_constant_ranges: &[],
			})),
			vertex: VertexState {
				module: &device.create_shader_module(&include_wgsl!("shaders/fullscreen.wgsl")),
				entry_point: "main",
				buffers: &[],
			},
			primitive: Default::default(),
			depth_stencil: None,
			multisample: Default::default(),
			fragment: Some(FragmentState {
				module: &device.create_shader_module(&include_wgsl!("shaders/render.wgsl")),
				entry_point: "main",
				targets: &[ColorTargetState::from(options.output_format)],
			}),
			multiview: None,
		});

		let group = Self::make_bind_group(device, &layout, &cbuffer, &cache);

		Ok(Self {
			cache,
			cbuffer,
			aspect_ratio,
			pipeline,
			group,
			layout,
		})
	}

	pub fn render(
		&mut self, options: &FrameOptions, device: &Device, queue: &Queue, view: &TextureView,
		encoder: &mut EncoderProfiler,
	) {
		tracy::zone!("Map Render");

		if let UploadStatus::Resized = self.cache.populate_tiles(device, queue, options.range) {
			self.group = Self::make_bind_group(device, &self.layout, &self.cbuffer, &self.cache);
		}

		{
			tracy::zone!("Tile Status Clear");

			encoder.clear_buffer(self.cache.tile_status(), 0, None);
			queue.write_buffer(
				&self.cbuffer,
				0,
				&Self::get_cbuffer_data(&self.cache, self.aspect_ratio, options),
			);
		}

		tracy::zone!("Render");

		let mut pass = tracy::wgpu_render_pass!(
			encoder,
			RenderPassDescriptor {
				label: Some("Map Render Pass"),
				color_attachments: &[RenderPassColorAttachment {
					view,
					resolve_target: None,
					ops: Operations {
						load: LoadOp::Clear(Color::BLACK),
						store: true,
					},
				}],
				depth_stencil_attachment: None,
			}
		);
		pass.set_pipeline(&self.pipeline);
		pass.set_bind_group(0, &self.group, &[]);
		pass.draw(0..3, 0..1);
	}

	pub fn resize(&mut self, width: u32, height: u32) { self.aspect_ratio = width as f32 / height as f32; }

	fn make_bind_group(device: &Device, layout: &BindGroupLayout, cbuffer: &Buffer, cache: &TileCache) -> BindGroup {
		device.create_bind_group(&BindGroupDescriptor {
			label: Some("Map Render Bind Group"),
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
				BindGroupEntry {
					binding: 4,
					resource: BindingResource::TextureView(&cache.hillshade()),
				},
			],
		})
	}

	fn get_cbuffer_data(cache: &TileCache, aspect_ratio: f32, options: &FrameOptions) -> [u8; Self::CBUFFER_SIZE as _] {
		let mut data = [0; Self::CBUFFER_SIZE as _];

		data[0..4].copy_from_slice(&options.position.lat.to_radians().to_le_bytes());
		data[4..8].copy_from_slice(&options.position.lon.to_radians().to_le_bytes());

		data[16..20].copy_from_slice(&options.range.vertical_radians().to_le_bytes());
		data[20..24].copy_from_slice(&aspect_ratio.to_le_bytes());
		data[24..28].copy_from_slice(&cache.tile_size_for_range(options.range).to_le_bytes());
		data[28..32].copy_from_slice(&(360. - options.heading).to_radians().to_le_bytes());
		data[32..36].copy_from_slice(&options.altitude.to_le_bytes());

		data
	}
}
