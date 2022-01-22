use std::num::NonZeroU64;

use glam::{Vec2, Vec3, Vec3A};
pub use wgpu;
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
	BufferBinding,
	BufferBindingType,
	BufferDescriptor,
	BufferUsages,
	Color,
	CommandEncoderDescriptor,
	ComputePassDescriptor,
	ComputePipeline,
	ComputePipelineDescriptor,
	Device,
	Extent3d,
	LoadOp,
	Operations,
	PipelineLayoutDescriptor,
	Queue,
	RenderPassColorAttachment,
	RenderPassDescriptor,
	ShaderStages,
	StorageTextureAccess,
	Texture,
	TextureAspect,
	TextureDescriptor,
	TextureDimension,
	TextureFormat,
	TextureUsages,
	TextureView,
	TextureViewDescriptor,
	TextureViewDimension,
};

#[repr(C)]
struct Uniform {
	camera_origin: Vec3A,
	camera_up: Vec3A,
	camera_right: Vec3,
	pixel_delta: f32,
	screen: Vec2,
}

pub struct Renderer {
	device: Device,
	queue: Queue,
	uniform: Buffer,
	heightmap: Texture,
	heightmap_view: TextureView,
	height_pipeline: ComputePipeline,
	bind_group: BindGroup,
	bind_group_layout: BindGroupLayout,
	size: (u32, u32),
}

impl Renderer {
	pub fn new(device: Device, queue: Queue, size: (u32, u32)) -> Self {
		let uniform = create_uniform(&device);
		let (heightmap, heightmap_view) = create_heightmap(&device, size);
		let bind_group_layout = create_bind_group_layout(&device);

		Self {
			height_pipeline: create_height_pipeline(&device, &bind_group_layout),
			bind_group: create_bind_group(&device, &bind_group_layout, &uniform, &heightmap_view),
			bind_group_layout,
			uniform,
			heightmap,
			heightmap_view,
			device,
			queue,
			size,
		}
	}

	pub fn render(&mut self, image: TextureView) {
		let mut buffer = self
			.device
			.create_command_encoder(&CommandEncoderDescriptor { label: Some("Render") });

		{
			let screen = Vec2::new(self.size.0 as f32, self.size.1 as f32);
			let range = 640.0 * 1852.0 * 1.8 * 2.0;
			let pixel_delta = range / screen.y;

			self.queue.write_buffer(&self.uniform, 0, unsafe {
				std::slice::from_raw_parts(
					&Uniform {
						camera_origin: Vec3A::new(0.0, 0.0, -6371000.0 * 1.5),
						camera_up: Vec3A::new(0.0, 1.0, 0.0),
						camera_right: Vec3::new(1.0, 0.0, 0.0),
						pixel_delta,
						screen,
					} as *const Uniform as *const _,
					std::mem::size_of::<Uniform>(),
				)
			});

			let mut pass = buffer.begin_compute_pass(&ComputePassDescriptor {
				label: Some("Height map"),
			});
			pass.set_pipeline(&self.height_pipeline);
			pass.set_bind_group(0, &self.bind_group, &[]);
			pass.dispatch(self.size.0, self.size.1, 1);
		}

		{
			let _ = buffer.begin_render_pass(&RenderPassDescriptor {
				label: Some("Render"),
				color_attachments: &[RenderPassColorAttachment {
					view: &image,
					resolve_target: None,
					ops: Operations {
						load: LoadOp::Clear(Color::BLACK),
						store: true,
					},
				}],
				depth_stencil_attachment: None,
			});
		}

		self.queue.submit([buffer.finish()])
	}

	pub fn resize(&mut self, size: (u32, u32)) {
		let (heightmap, heightmap_view) = create_heightmap(&self.device, size);
		self.heightmap = heightmap;
		self.heightmap_view = heightmap_view;
		self.size = size;
		self.bind_group = create_bind_group(
			&self.device,
			&self.bind_group_layout,
			&self.uniform,
			&self.heightmap_view,
		);
	}

	pub fn device(&self) -> &Device { &self.device }
}

fn create_uniform(device: &Device) -> Buffer {
	device.create_buffer(&BufferDescriptor {
		label: Some("Uniform"),
		size: (std::mem::size_of::<Uniform>() as u64).max(64),
		usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
		mapped_at_creation: false,
	})
}

fn create_heightmap(device: &Device, (width, height): (u32, u32)) -> (Texture, TextureView) {
	let texture = device.create_texture(&TextureDescriptor {
		label: Some("Heightmap"),
		size: Extent3d {
			width,
			height,
			depth_or_array_layers: 1,
		},
		mip_level_count: 1,
		sample_count: 1,
		dimension: TextureDimension::D2,
		format: TextureFormat::R32Float,
		usage: TextureUsages::STORAGE_BINDING,
	});

	let view = texture.create_view(&TextureViewDescriptor {
		label: Some("Heightmap"),
		format: None,
		dimension: None,
		aspect: TextureAspect::All,
		base_mip_level: 0,
		mip_level_count: None,
		base_array_layer: 0,
		array_layer_count: None,
	});

	(texture, view)
}

fn create_bind_group_layout(device: &Device) -> BindGroupLayout {
	device.create_bind_group_layout(&BindGroupLayoutDescriptor {
		label: Some("Bind group"),
		entries: &[
			BindGroupLayoutEntry {
				binding: 0,
				visibility: ShaderStages::COMPUTE,
				ty: BindingType::Buffer {
					ty: BufferBindingType::Uniform,
					has_dynamic_offset: false,
					min_binding_size: Some(NonZeroU64::new(64).unwrap()),
				},
				count: None,
			},
			BindGroupLayoutEntry {
				binding: 1,
				visibility: ShaderStages::COMPUTE,
				ty: BindingType::StorageTexture {
					access: StorageTextureAccess::WriteOnly,
					format: TextureFormat::R32Float,
					view_dimension: TextureViewDimension::D2,
				},
				count: None,
			},
		],
	})
}

fn create_bind_group(
	device: &Device, layout: &BindGroupLayout, uniform: &Buffer, height_map: &TextureView,
) -> BindGroup {
	device.create_bind_group(&BindGroupDescriptor {
		label: Some("Bind group"),
		layout,
		entries: &[
			BindGroupEntry {
				binding: 0,
				resource: BindingResource::Buffer(BufferBinding {
					buffer: uniform,
					offset: 0,
					size: None,
				}),
			},
			BindGroupEntry {
				binding: 1,
				resource: BindingResource::TextureView(&height_map),
			},
		],
	})
}

fn create_height_pipeline(device: &Device, layout: &BindGroupLayout) -> ComputePipeline {
	device.create_compute_pipeline(&ComputePipelineDescriptor {
		label: Some("Height map"),
		layout: Some(&device.create_pipeline_layout(&PipelineLayoutDescriptor {
			label: Some("Height map"),
			bind_group_layouts: &[layout],
			push_constant_ranges: &[],
		})),
		module: &device.create_shader_module(&include_wgsl!("heightmap.wgsl")),
		entry_point: "main",
	})
}
