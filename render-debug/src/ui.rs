use egui::{ComboBox, Context, DragValue, Window};
use render::{
	range::{Mode, Range},
	LatLon,
	Renderer,
};
use tracy::wgpu::EncoderProfiler;
use wgpu::{
	Color,
	Device,
	LoadOp,
	Operations,
	Queue,
	RenderPassColorAttachment,
	RenderPassDescriptor,
	TextureFormat,
	TextureView,
};

pub struct Ui {
	data_path: String,
	position: LatLon,
	range: Range,
	heading: f32,
	azimuth: f32,
	altitude: f32,
	aircraft_altitude: f32,
	renderer: Option<Renderer>,
}

impl Ui {
	pub fn new() -> Self {
		Self {
			data_path: String::new(),
			position: LatLon { lat: 0., lon: 0. },
			range: Range::Nm10,
			heading: 0.,
			azimuth: 315.,
			altitude: 45.,
			renderer: None,
			aircraft_altitude: 1000.,
		}
	}

	pub fn update<'a>(
		&'a mut self, ctx: &Context, device: &Device, queue: &Queue, encoder: &mut EncoderProfiler, view: &TextureView,
		format: TextureFormat,
	) {
		Window::new("Settings").show(ctx, |ui| {
			tracy::zone!("UI Description");

			ui.horizontal(|ui| {
				ui.label("Data");
				ui.text_edit_singleline(&mut self.data_path);
				if ui.button("...").clicked() {
					if let Some(data) = rfd::FileDialog::new().pick_folder() {
						if let Some(data_s) = data.to_str() {
							self.data_path = data_s.into();
							let renderer = match Renderer::new(device, format, data) {
								Ok(x) => x,
								Err(e) => {
									log::error!("{}", e);
									return;
								},
							};
							self.renderer = Some(renderer);
						}
					}
				}
			});

			ui.horizontal(|ui| {
				ui.label("Lat");
				ui.add(
					DragValue::new(&mut self.position.lat)
						.clamp_range(-90.0..=90.0)
						.speed(0.1),
				);
				ui.label("Lon");
				ui.add(
					DragValue::new(&mut self.position.lon)
						.clamp_range(-180.0..=180.0)
						.speed(0.1),
				);
			});

			ui.horizontal(|ui| {
				ui.label("Range");

				ComboBox::from_label("")
					.selected_text(self.range.to_str())
					.show_ui(ui, |ui| {
						fn range_selector(ui: &mut egui::Ui, set: &mut Range, range: Range) {
							ui.selectable_value(set, range, range.to_str());
						}

						range_selector(ui, &mut self.range, Range::Nm2);
						range_selector(ui, &mut self.range, Range::Nm5);
						range_selector(ui, &mut self.range, Range::Nm10);
						range_selector(ui, &mut self.range, Range::Nm20);
						range_selector(ui, &mut self.range, Range::Nm40);
						range_selector(ui, &mut self.range, Range::Nm80);
						range_selector(ui, &mut self.range, Range::Nm160);
						range_selector(ui, &mut self.range, Range::Nm320);
						range_selector(ui, &mut self.range, Range::Nm640);
					});
			});

			ui.horizontal(|ui| {
				ui.label("Heading");
				ui.add(DragValue::new(&mut self.heading).clamp_range(0.0..=360.0).speed(1.0));
			});

			ui.horizontal(|ui| {
				ui.label("Azimuth");
				ui.add(DragValue::new(&mut self.azimuth).clamp_range(0.0..=360.0).speed(1.0));
				ui.label("Altitude");
				ui.add(DragValue::new(&mut self.altitude).clamp_range(0.0..=90.0).speed(1.0));
			});

			ui.horizontal(|ui| {
				ui.label("Aircraft Altitude");
				ui.add(
					DragValue::new(&mut self.aircraft_altitude)
						.clamp_range(0.0..=50000.0)
						.speed(100.0),
				);
			});
		});

		tracy::zone!("Map Render");
		if let Some(renderer) = self.renderer.as_mut() {
			renderer.render(
				self.position,
				self.range,
				self.heading,
				self.azimuth,
				self.altitude,
				self.aircraft_altitude,
				Mode::FullPage,
				device,
				queue,
				encoder,
				|encoder| {
					tracy::wgpu_render_pass!(
						encoder,
						RenderPassDescriptor {
							label: Some("Map Render"),
							color_attachments: &[RenderPassColorAttachment {
								view,
								resolve_target: None,
								ops: Operations {
									load: LoadOp::Clear(Color::BLACK),
									store: true,
								}
							}],
							depth_stencil_attachment: None,
						}
					)
				},
			);
		}
	}
}
