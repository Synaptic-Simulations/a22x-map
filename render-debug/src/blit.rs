use tracy::wgpu::EncoderProfiler;
use wgpu::{
	include_wgsl,
	AddressMode,
	BindGroup,
	BindGroupDescriptor,
	BindGroupEntry,
	BindGroupLayoutDescriptor,
	BindGroupLayoutEntry,
	BindingResource,
	BindingType,
	Color,
	ColorTargetState,
	Device,
	FilterMode,
	FragmentState,
	LoadOp,
	Operations,
	PipelineLayoutDescriptor,
	RenderPassColorAttachment,
	RenderPassDescriptor,
	RenderPipeline,
	RenderPipelineDescriptor,
	SamplerBindingType,
	SamplerDescriptor,
	ShaderStages,
	TextureSampleType,
	TextureView,
	TextureViewDimension,
	VertexState,
};

use crate::TextureFormat;

pub struct Blitter {
	pipeline: RenderPipeline,
	group: BindGroup,
}

impl Blitter {
	pub fn new(device: &Device, from: &TextureView, to_format: TextureFormat) -> Self {
		let layout = &device.create_bind_group_layout(&BindGroupLayoutDescriptor {
			label: Some("Blit Layout"),
			entries: &[
				BindGroupLayoutEntry {
					binding: 0,
					visibility: ShaderStages::FRAGMENT,
					ty: BindingType::Sampler(SamplerBindingType::Filtering),
					count: None,
				},
				BindGroupLayoutEntry {
					binding: 1,
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
		let module = &device.create_shader_module(&include_wgsl!("blit.wgsl"));
		let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
			label: Some("Blit Pipeline"),
			layout: Some(&device.create_pipeline_layout(&PipelineLayoutDescriptor {
				label: Some("Blit Layout"),
				bind_group_layouts: &[layout],
				push_constant_ranges: &[],
			})),
			vertex: VertexState {
				module,
				entry_point: "vertex",
				buffers: &[],
			},
			primitive: Default::default(),
			depth_stencil: None,
			multisample: Default::default(),
			fragment: Some(FragmentState {
				module,
				entry_point: "pixel",
				targets: &[ColorTargetState::from(to_format)],
			}),
			multiview: None,
		});
		let group = device.create_bind_group(&BindGroupDescriptor {
			label: Some("Blit Bind Group"),
			layout,
			entries: &[
				BindGroupEntry {
					binding: 0,
					resource: BindingResource::Sampler(&device.create_sampler(&SamplerDescriptor {
						label: Some("Final Sampler"),
						address_mode_u: AddressMode::ClampToEdge,
						address_mode_v: AddressMode::ClampToEdge,
						address_mode_w: AddressMode::ClampToEdge,
						mag_filter: FilterMode::Linear,
						min_filter: FilterMode::Linear,
						mipmap_filter: FilterMode::Linear,
						lod_min_clamp: 0.,
						lod_max_clamp: 0.,
						compare: None,
						anisotropy_clamp: None,
						border_color: None,
					})),
				},
				BindGroupEntry {
					binding: 1,
					resource: BindingResource::TextureView(from),
				},
			],
		});

		Self { pipeline, group }
	}

	pub fn blit(&self, encoder: &mut EncoderProfiler, to: &TextureView) {
		let mut pass = tracy::wgpu_render_pass!(
			encoder,
			RenderPassDescriptor {
				label: Some("Blit"),
				color_attachments: &[RenderPassColorAttachment {
					view: to,
					resolve_target: None,
					ops: Operations {
						load: LoadOp::Clear(Color::BLACK),
						store: true,
					}
				}],
				depth_stencil_attachment: None,
			}
		);

		pass.set_pipeline(&self.pipeline);
		pass.set_bind_group(0, &self.group, &[]);
		pass.draw(0..3, 0..1);
	}
}
