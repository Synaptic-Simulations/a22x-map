use render::{
	wgpu::{
		Backends,
		DeviceDescriptor,
		Instance,
		PowerPreference,
		PresentMode,
		RequestAdapterOptions,
		SurfaceConfiguration,
		TextureUsages,
		TextureViewDescriptor,
	},
	Renderer,
};
use winit::{
	event::{Event, WindowEvent},
	event_loop::{ControlFlow, EventLoop},
	window::WindowBuilder,
};

fn main() {
	env_logger::init();
	let event_loop = EventLoop::new();
	let window = WindowBuilder::new()
		.with_title("map-render")
		.with_visible(false)
		.build(&event_loop)
		.unwrap();

	let instance = Instance::new(Backends::all());
	let surface = unsafe { instance.create_surface(&window) };
	let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
		power_preference: PowerPreference::default(),
		compatible_surface: Some(&surface),
		force_fallback_adapter: false,
	}))
	.unwrap();

	let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor::default(), None)).unwrap();

	let size = window.inner_size();
	let mut config = SurfaceConfiguration {
		usage: TextureUsages::RENDER_ATTACHMENT,
		format: surface.get_preferred_format(&adapter).unwrap(),
		width: size.width,
		height: size.height,
		present_mode: PresentMode::Fifo,
	};
	surface.configure(&device, &config);

	let mut renderer = Renderer::new(device, queue, (size.width, size.height));

	window.set_visible(true);
	event_loop.run(move |event, _, control_flow| match event {
		Event::MainEventsCleared => {
			let output = surface.get_current_texture().unwrap();
			let view = output.texture.create_view(&TextureViewDescriptor {
				label: Some("Backbuffer"),
				..Default::default()
			});

			renderer.render(view);

			output.present();
		},
		Event::WindowEvent { ref event, window_id } if window_id == window.id() => match event {
			WindowEvent::Resized(size) => {
				if size.width > 0 && size.height > 0 {
					config.width = size.width;
					config.height = size.height;
					surface.configure(renderer.device(), &config);
					renderer.resize((size.width, size.height));
				}
			},
			WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
			_ => {},
		},
		_ => {},
	});
}
