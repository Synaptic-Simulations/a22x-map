use std::{
	num::NonZeroU64,
	ops::{Add, DerefMut, Sub},
	path::PathBuf,
};

use geo::LoadError;
use wgpu::{
	include_spirv_raw,
	BindGroup,
	BindGroupDescriptor,
	BindGroupEntry,
	BindGroupLayout,
	BindGroupLayoutDescriptor,
	BindGroupLayoutEntry,
	BindingResource,
	BindingType,
	Buffer,
	BufferBinding,
	BufferBindingType,
	BufferDescriptor,
	BufferUsages,
	ColorTargetState,
	CommandEncoder,
	Device,
	FragmentState,
	PipelineLayoutDescriptor,
	PrimitiveState,
	PrimitiveTopology,
	Queue,
	RenderPass,
	RenderPipeline,
	RenderPipelineDescriptor,
	ShaderStages,
	TextureFormat,
	TextureSampleType,
	TextureViewDimension,
	VertexState,
};

use crate::{
	range::{Mode, Range},
	tile_cache::TileCache,
};

pub mod range;
mod tile_cache;

/// A polar coordinate, in degrees.
#[derive(Copy, Clone, Debug)]
pub struct LatLon {
	pub lat: f32,
	pub lon: f32,
}

impl LatLon {
	pub fn ceil(self) -> Self {
		LatLon {
			lat: self.lat.ceil(),
			lon: self.lon.ceil(),
		}
	}

	pub fn floor(self) -> Self {
		LatLon {
			lat: self.lat.floor(),
			lon: self.lon.floor(),
		}
	}
}

impl Add for LatLon {
	type Output = LatLon;

	fn add(self, other: LatLon) -> LatLon {
		LatLon {
			lat: self.lat + other.lat,
			lon: self.lon + other.lon,
		}
	}
}

impl Sub for LatLon {
	type Output = LatLon;

	fn sub(self, other: LatLon) -> LatLon {
		LatLon {
			lat: self.lat - other.lat,
			lon: self.lon - other.lon,
		}
	}
}

pub struct Renderer {
	pipeline: RenderPipeline,
	group: BindGroup,
	group_layout: BindGroupLayout,
	cbuffer: Buffer,
	cache: TileCache,
}

impl Renderer {
	pub fn new(device: &Device, format: TextureFormat, data_path: PathBuf) -> Result<Self, LoadError> {
		let sets = std::fs::read_to_string(data_path.join("_meta"))?;
		let datasets = sets.lines().map(|line| data_path.join(line)).collect();
		let cache = TileCache::new(device, datasets)?;

		let vertex = unsafe { device.create_shader_module_spirv(&include_spirv_raw!(env!("FullscreenVS.hlsl"))) };
		let fragment = unsafe { device.create_shader_module_spirv(&include_spirv_raw!(env!("RenderPS.hlsl"))) };

		let group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
			label: Some("Map Render Bind Group"),
			entries: &[
				BindGroupLayoutEntry {
					binding: 0,
					visibility: ShaderStages::FRAGMENT,
					ty: BindingType::Buffer {
						ty: BufferBindingType::Uniform,
						has_dynamic_offset: false,
						min_binding_size: Some(NonZeroU64::new(28).unwrap()),
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

		let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
			label: Some("Map Render Pipeline"),
			layout: Some(&device.create_pipeline_layout(&PipelineLayoutDescriptor {
				label: Some("Map Render Pipeline Layout"),
				bind_group_layouts: &[&group_layout],
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
				module: &fragment,
				entry_point: "Main",
				targets: &[ColorTargetState::from(format)],
			}),
			multiview: None,
		});

		let cbuffer = device.create_buffer(&BufferDescriptor {
			label: Some("Map Render Constant Buffer"),
			size: 36,
			usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		let group = Self::make_bind_group(device, &group_layout, &cbuffer, &cache);

		Ok(Self {
			pipeline,
			group,
			group_layout,
			cbuffer,
			cache,
		})
	}

	pub fn render<'a, T: DerefMut<Target = CommandEncoder>, U: DerefMut<Target = RenderPass<'a>>>(
		&'a mut self, pos: LatLon, range: Range, mode: Mode, device: &Device, queue: &Queue, encoder: &'a mut T,
		pass: impl FnOnce(&'a mut T) -> U,
	) {
		encoder.clear_buffer(self.cache.tile_status(), 0, None);
		queue.write_buffer(&self.cbuffer, 0, &Self::get_cbuffer_data(&self.cache, pos, range, mode));

		if self.cache.populate_tiles(device, queue, range) {
			self.group = Self::make_bind_group(device, &self.group_layout, &self.cbuffer, &self.cache);
		}

		{
			let mut pass = pass(encoder);
			pass.set_pipeline(&self.pipeline);
			pass.set_bind_group(0, &self.group, &[]);
			pass.draw(0..3, 0..1);
		}
	}

	fn make_bind_group(device: &Device, layout: &BindGroupLayout, cbuffer: &Buffer, cache: &TileCache) -> BindGroup {
		device.create_bind_group(&BindGroupDescriptor {
			label: Some("Map Render Bind Group"),
			layout,
			entries: &[
				BindGroupEntry {
					binding: 0,
					resource: BindingResource::Buffer(BufferBinding {
						buffer: &cbuffer,
						offset: 0,
						size: None,
					}),
				},
				BindGroupEntry {
					binding: 1,
					resource: BindingResource::TextureView(cache.tile_map()),
				},
				BindGroupEntry {
					binding: 2,
					resource: BindingResource::Buffer(BufferBinding {
						buffer: cache.tile_status(),
						offset: 0,
						size: None,
					}),
				},
				BindGroupEntry {
					binding: 3,
					resource: BindingResource::TextureView(&cache.atlas()),
				},
			],
		})
	}

	fn get_cbuffer_data(cache: &TileCache, pos: LatLon, range: Range, mode: Mode) -> [u8; 28] {
		let mut data = [0; 28];

		data[0..4].copy_from_slice(&pos.lat.to_radians().to_le_bytes());
		data[4..8].copy_from_slice(&pos.lon.to_radians().to_le_bytes());

		data[16..20].copy_from_slice(&range.horizontal_radians(mode).to_le_bytes());
		data[20..24].copy_from_slice(&range.vertical_radians().to_le_bytes());

		data[24..28].copy_from_slice(&cache.tile_size_for_range(range).to_le_bytes());

		data
	}
}
