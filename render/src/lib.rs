use std::{
	num::NonZeroU64,
	ops::{Add, Sub},
	path::PathBuf,
};

use geo::LoadError;
use wgpu::{
	include_spirv_raw,
	util::{BufferInitDescriptor, DeviceExt},
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
	BufferUsages,
	Color,
	ColorTargetState,
	CommandEncoder,
	Device,
	Extent3d,
	FragmentState,
	LoadOp,
	Operations,
	PipelineLayoutDescriptor,
	PrimitiveState,
	PrimitiveTopology,
	Queue,
	RenderPass,
	RenderPassColorAttachment,
	RenderPassDescriptor,
	RenderPipeline,
	RenderPipelineDescriptor,
	ShaderStages,
	Texture,
	TextureDescriptor,
	TextureDimension,
	TextureFormat,
	TextureSampleType,
	TextureUsages,
	TextureView,
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
	pub fn new(
		device: &Device, queue: &Queue, format: TextureFormat, data_path: PathBuf, pos: LatLon, range: Range,
		mode: Mode,
	) -> Result<Self, LoadError> {
		let sets = std::fs::read_to_string(data_path.join("_meta"))?;
		let datasets = sets.lines().map(|line| data_path.join(line)).collect();
		let cache = TileCache::new(device, queue, datasets, pos, range, mode)?;

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
						min_binding_size: Some(NonZeroU64::new(48).unwrap()),
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

		let data = Self::get_cbuffer_data(&cache, pos, range, mode);
		let cbuffer = device.create_buffer_init(&BufferInitDescriptor {
			label: Some("Map Render Constant Buffer"),
			contents: &data,
			usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
		});

		let group = device.create_bind_group(&BindGroupDescriptor {
			label: Some("Map Render Bind Group"),
			layout: &group_layout,
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
					resource: BindingResource::TextureView(&cache.atlas()),
				},
			],
		});

		Ok(Self {
			pipeline,
			group,
			group_layout,
			cbuffer,
			cache,
		})
	}

	pub fn render<'a>(&'a mut self, pass: &mut RenderPass<'a>) {
		pass.set_pipeline(&self.pipeline);
		pass.set_bind_group(0, &self.group, &[]);
		pass.draw(0..3, 0..1);
	}

	fn get_cbuffer_data(cache: &TileCache, pos: LatLon, range: Range, mode: Mode) -> [u8; 48] {
		let mut data = [0; 48];

		let float1 = &mut data[0..4];
		float1.copy_from_slice(&pos.lat.to_radians().to_le_bytes());
		let float2 = &mut data[4..8];
		float2.copy_from_slice(&pos.lon.to_radians().to_le_bytes());

		let float3 = &mut data[16..20];
		float3.copy_from_slice(&range.horizontal_radians(mode).to_le_bytes());
		let float4 = &mut data[20..24];
		float4.copy_from_slice(&range.vertical_radians().to_le_bytes());

		let (width, height) = cache.atlas_resolution();
		let uint1 = &mut data[32..36];
		uint1.copy_from_slice(&width.to_le_bytes());
		let uint2 = &mut data[36..40];
		uint2.copy_from_slice(&height.to_le_bytes());
		let uint3 = &mut data[40..44];
		uint3.copy_from_slice(&cache.tile_size_for_range(range).to_le_bytes());

		data
	}
}
