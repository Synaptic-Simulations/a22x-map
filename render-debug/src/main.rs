use std::time::Instant;

use egui::FontDefinitions;
use egui_wgpu_backend::ScreenDescriptor;
use egui_winit_platform::{Platform, PlatformDescriptor};
use futures_lite::future::block_on;
use tracing_subscriber::prelude::*;
use tracy::{tracing::TracyLayer, wgpu::ProfileContext};
use wgpu::{
	Backends,
	CommandEncoderDescriptor,
	DeviceDescriptor,
	Extent3d,
	Features,
	Instance,
	LoadOp,
	Maintain,
	Operations,
	PowerPreference,
	PresentMode,
	RenderPassColorAttachment,
	RenderPassDescriptor,
	RequestAdapterOptions,
	SurfaceConfiguration,
	TextureDescriptor,
	TextureDimension,
	TextureFormat,
	TextureUsages,
};
use winit::{
	dpi::PhysicalSize,
	event::{Event, WindowEvent},
	event_loop::{ControlFlow, EventLoop},
	window::WindowBuilder,
};

use crate::{blit::Blitter, ui::Ui};

mod blit;
mod ui;

fn main() {
	env_logger::init();
	let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry().with(TracyLayer)).unwrap();

	let event_loop = EventLoop::new();
	let window = WindowBuilder::new()
		.with_title("map-render")
		.with_visible(false)
		.with_inner_size(PhysicalSize {
			width: 1480,
			height: 800,
		})
		.build(&event_loop)
		.unwrap();

	let instance = Instance::new(Backends::all());
	let surface = unsafe { instance.create_surface(&window) };
	let adapter = block_on(instance.request_adapter(&RequestAdapterOptions {
		power_preference: PowerPreference::default(),
		compatible_surface: Some(&surface),
		force_fallback_adapter: false,
	}))
	.unwrap();

	let timestamp_query = adapter.features().contains(Features::TIMESTAMP_QUERY);

	let (device, queue) = block_on(adapter.request_device(
		&DeviceDescriptor {
			label: Some("Device"),
			features: if timestamp_query {
				Features::TIMESTAMP_QUERY
			} else {
				Features::empty()
			},
			limits: Default::default(),
		},
		None,
	))
	.unwrap();

	let mut profiler = ProfileContext::with_enabled_and_name("GPU", &adapter, &device, &queue, 2, timestamp_query);
	let mut ui = Ui::new();

	let size = window.inner_size();
	let mut config = SurfaceConfiguration {
		usage: TextureUsages::RENDER_ATTACHMENT,
		format: surface.get_preferred_format(&adapter).unwrap(),
		width: size.width,
		height: size.height,
		present_mode: PresentMode::Fifo,
	};
	surface.configure(&device, &config);

	let map = device.create_texture(&TextureDescriptor {
		label: Some("Map"),
		size: Extent3d {
			width: config.width,
			height: config.height,
			depth_or_array_layers: 1,
		},
		mip_level_count: 1,
		sample_count: 1,
		dimension: TextureDimension::D2,
		format: TextureFormat::Rgba8Unorm,
		usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
	});
	let map_view = map.create_view(&Default::default());
	let blitter = Blitter::new(&device, &map_view, config.format);

	let mut platform = Platform::new(PlatformDescriptor {
		physical_width: size.width,
		physical_height: size.height,
		scale_factor: window.scale_factor(),
		font_definitions: FontDefinitions::default(),
		style: Default::default(),
	});
	let mut egui_pass = egui_wgpu_backend::RenderPass::new(&device, config.format, 1);

	window.set_visible(true);
	let start_time = Instant::now();
	event_loop.run(move |event, _, control_flow| {
		platform.handle_event(&event);
		match event {
			Event::MainEventsCleared => window.request_redraw(),
			Event::RedrawRequested(_) => {
				let (texture, view) = {
					tracy::zone!("Acquire Image");

					let texture = match surface.get_current_texture() {
						Ok(tex) => tex,
						Err(_) => return,
					};
					let view = texture.texture.create_view(&Default::default());

					(texture, view)
				};

				let mut encoder =
					tracy::wgpu_command_encoder!(device, profiler, CommandEncoderDescriptor { label: Some("Exec") });

				platform.update_time(start_time.elapsed().as_secs_f64());
				platform.begin_frame();

				let context = platform.context();
				{
					ui.update(
						&context,
						&device,
						&queue,
						&mut encoder,
						&map_view,
						TextureFormat::Rgba8Unorm,
					);
					blitter.blit(&mut encoder, &view);
				}

				let (screen_descriptor, tesselated) = {
					tracy::zone!("UI Tesselation");

					let output = platform.end_frame(Some(&window));
					let screen_descriptor = ScreenDescriptor {
						physical_height: config.height,
						physical_width: config.width,
						scale_factor: window.scale_factor() as _,
					};
					let tesselated = context.tessellate(output.shapes);
					egui_pass.update_buffers(&device, &queue, &tesselated, &screen_descriptor);
					egui_pass.add_textures(&device, &queue, &output.textures_delta).unwrap();
					egui_pass.remove_textures(output.textures_delta).unwrap();

					(screen_descriptor, tesselated)
				};

				{
					tracy::zone!("UI Render");

					let mut render_pass = tracy::wgpu_render_pass!(
						encoder,
						RenderPassDescriptor {
							label: Some("UI"),
							color_attachments: &[RenderPassColorAttachment {
								view: &view,
								resolve_target: None,
								ops: Operations {
									load: LoadOp::Load,
									store: true,
								},
							}],
							depth_stencil_attachment: None,
						}
					);
					egui_pass
						.execute_with_renderpass(&mut render_pass, &tesselated, &screen_descriptor)
						.unwrap();
				}

				tracy::zone!("Submit");
				queue.submit([encoder.finish()]);

				profiler.end_frame(&device, &queue);
				texture.present();

				{
					tracy::zone!("GPU Sync");
					device.poll(Maintain::Wait);
					block_on(queue.on_submitted_work_done());
				}

				tracy::frame!();
			},
			Event::WindowEvent { ref event, .. } => match event {
				WindowEvent::Resized(size) => {
					if size.width > 0 && size.height > 0 {
						config.width = size.width;
						config.height = size.height;
						surface.configure(&device, &config);
						ui.resize(size.width, size.height);
					}
				},
				WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
				_ => {},
			},
			_ => {},
		}
	});
}
