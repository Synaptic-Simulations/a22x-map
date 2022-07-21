use std::{
	error::Error,
	io::Write,
	num::{NonZeroU32, NonZeroUsize},
	path::PathBuf,
	sync::Mutex,
};

use dashmap::DashMap;
use futures_lite::future::block_on;
use png::{BitDepth, ColorType, Encoder};
use render::{FrameOptions, LatLon, Renderer, RendererOptions};
use rouille::{try_or_400::ErrJson, Request, Response};
use tracy::wgpu::ProfileContext;
use url::Url;

struct RenderData {
	renderer: Renderer,
	res: (u32, u32),
	texture: wgpu::Texture,
	readback_buffer: wgpu::Buffer,
	stride: NonZeroU32,
}

impl RenderData {
	fn new(device: &wgpu::Device, path: PathBuf, width: u32, height: u32) -> Self {
		let renderer = Renderer::new(
			device,
			&RendererOptions {
				data_path: path,
				output_format: wgpu::TextureFormat::Rgba8UnormSrgb,
			},
		)
		.unwrap();
		let texture = device.create_texture(&wgpu::TextureDescriptor {
			label: None,
			size: wgpu::Extent3d {
				width,
				height,
				depth_or_array_layers: 1,
			},
			mip_level_count: 1,
			sample_count: 1,
			dimension: wgpu::TextureDimension::D2,
			format: wgpu::TextureFormat::Rgba8UnormSrgb,
			usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
		});

		let stride = 4 * width;
		let stride = NonZeroU32::new((stride + 256 - 1) & !255).unwrap();
		let buffer = device.create_buffer(&wgpu::BufferDescriptor {
			label: None,
			size: (stride.get() * height) as _,
			usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
			mapped_at_creation: false,
		});

		Self {
			renderer,
			res: (width, height),
			texture,
			readback_buffer: buffer,
			stride,
		}
	}
}

fn main() {
	let path = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| {
		println!("Usage: {} <path>", std::env::args().nth(0).unwrap());
		std::process::exit(1);
	}));

	let instance = wgpu::Instance::new(wgpu::Backends::all());
	let adapter = block_on(instance.request_adapter(&Default::default())).unwrap();

	let timestamp_query = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);

	let (device, queue) = block_on(adapter.request_device(
		&wgpu::DeviceDescriptor {
			label: Some("Device"),
			features: if timestamp_query {
				wgpu::Features::TIMESTAMP_QUERY
			} else {
				wgpu::Features::empty()
			},
			limits: Default::default(),
		},
		None,
	))
	.unwrap();

	let profiler = Mutex::new(ProfileContext::with_enabled_and_name(
		"GPU",
		&adapter,
		&device,
		&queue,
		1,
		timestamp_query,
	));
	let id_to_renderer: DashMap<u32, RenderData> = DashMap::new();

	rouille::start_server_with_pool(
		"0.0.0.0:42069",
		std::thread::available_parallelism().ok().map(NonZeroUsize::get),
		move |req| match (|req: &Request| -> Result<_, Box<dyn Error>> {
			let url = Url::parse(&format!("http://127.0.0.1{}", req.raw_url()))?;

			if url.path() != "/map.png" {
				return Ok(Response::empty_404());
			}

			let mut id = 0;
			let mut res = (0, 0);
			let mut pos = (0.0, 0.0);
			let mut heading = 0.0;
			let mut altitude = 0.0;
			let mut range = 1.0;
			for (key, val) in url.query_pairs() {
				match key.as_ref() {
					"id" => id = val.parse::<u32>()?,
					"res" => {
						let mut split = val.split(',');
						res.0 = split.next().ok_or("missing res x")?.parse()?;
						res.1 = split.next().ok_or("missing res y")?.parse()?;
					},
					"pos" => {
						let mut split = val.split(',');
						pos.0 = split.next().ok_or("missing pos lat")?.parse()?;
						pos.1 = split.next().ok_or("missing pos lon")?.parse()?;
					},
					"heading" => heading = val.parse()?,
					"range" => range = val.parse()?,
					"alt" => altitude = val.parse()?,
					_ => return Err(From::from("unknown query param")),
				}
			}

			let mut renderer = if let Some(mut renderer) = id_to_renderer.get_mut(&id) {
				if renderer.res != res {
					*renderer = RenderData::new(&device, path.clone(), res.0, res.1);
				}
				renderer
			} else {
				id_to_renderer.insert(id, RenderData::new(&device, path.clone(), res.0, res.1));
				id_to_renderer.get_mut(&id).unwrap()
			};

			{
				let mut profiler = profiler.lock().unwrap();
				let mut encoder = tracy::wgpu_command_encoder!(device, profiler, Default::default());

				let view = renderer.texture.create_view(&Default::default());
				let opts = FrameOptions {
					width: res.0,
					height: res.1,
					position: LatLon { lat: pos.0, lon: pos.1 },
					vertical_angle: range,
					heading,
					altitude,
				};
				renderer.renderer.render(&opts, &device, &queue, &view, &mut encoder);

				queue.submit([encoder.finish()]);
				let _ = queue.on_submitted_work_done();
				device.poll(wgpu::Maintain::Wait);

				let mut encoder = tracy::wgpu_command_encoder!(device, profiler, Default::default());
				renderer.renderer.render(&opts, &device, &queue, &view, &mut encoder);

				encoder.copy_texture_to_buffer(
					wgpu::ImageCopyTexture {
						texture: &renderer.texture,
						mip_level: 0,
						origin: wgpu::Origin3d::ZERO,
						aspect: wgpu::TextureAspect::All,
					},
					wgpu::ImageCopyBuffer {
						buffer: &renderer.readback_buffer,
						layout: wgpu::ImageDataLayout {
							offset: 0,
							bytes_per_row: Some(renderer.stride),
							rows_per_image: Some(NonZeroU32::new(res.1).unwrap()),
						},
					},
					wgpu::Extent3d {
						width: res.0,
						height: res.1,
						depth_or_array_layers: 1,
					},
				);

				queue.submit([encoder.finish()]);
			}

			let mut out: Vec<u8> = Vec::new();
			{
				let _ = renderer.readback_buffer.slice(..).map_async(wgpu::MapMode::Read);
				device.poll(wgpu::Maintain::Wait);
				let view = renderer.readback_buffer.slice(..).get_mapped_range();

				let mut encoder = Encoder::new(&mut out, res.0, res.1);
				encoder.set_color(ColorType::Rgba);
				encoder.set_depth(BitDepth::Eight);
				let mut enc = encoder.write_header().unwrap();
				let mut writer = enc.stream_writer().unwrap();
				let stride = renderer.stride.get() as usize;

				for i in 0..res.1 {
					let i = i as usize;
					writer.write(&view[i * stride..(i + 1) * stride]).unwrap();
				}
				writer.finish().unwrap();
				enc.finish().unwrap();
			}
			renderer.readback_buffer.unmap();

			Ok(Response::from_data("image/png", out))
		})(req)
		{
			Ok(x) => x,
			Err(e) => Response::json(&ErrJson::from_err(&*e)).with_status_code(400),
		},
	);
}
