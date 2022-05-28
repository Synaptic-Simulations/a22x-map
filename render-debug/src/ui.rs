use egui::{ComboBox, Context, DragValue, Window};
use render::{range::Range, FrameOptions, Renderer, RendererOptions};
use tracy::wgpu::EncoderProfiler;
use wgpu::{Device, Queue, TextureFormat, TextureView};

pub struct Ui {
	data_path: String,
	options: FrameOptions,
	renderer: Option<Renderer>,
}

impl Ui {
	pub fn new() -> Self {
		Self {
			data_path: String::new(),
			options: FrameOptions::default(),
			renderer: None,
		}
	}

	pub fn update<'a>(
		&'a mut self, ctx: &Context, device: &Device, queue: &Queue, encoder: &mut EncoderProfiler, view: &TextureView,
		format: TextureFormat, resolution: (u32, u32),
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
							let renderer = match Renderer::new(
								device,
								&RendererOptions {
									data_path: data,
									width: resolution.0,
									height: resolution.1,
									output_format: format,
								},
							) {
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
					DragValue::new(&mut self.options.position.lat)
						.clamp_range(-90.0..=90.0)
						.speed(0.1),
				);
				ui.label("Lon");
				ui.add(
					DragValue::new(&mut self.options.position.lon)
						.clamp_range(-180.0..=180.0)
						.speed(0.1),
				);
			});

			ui.horizontal(|ui| {
				ui.label("Range");

				ComboBox::from_label("")
					.selected_text(self.options.range.to_str())
					.show_ui(ui, |ui| {
						fn range_selector(ui: &mut egui::Ui, set: &mut Range, range: Range) {
							ui.selectable_value(set, range, range.to_str());
						}

						range_selector(ui, &mut self.options.range, Range::Nm2);
						range_selector(ui, &mut self.options.range, Range::Nm5);
						range_selector(ui, &mut self.options.range, Range::Nm10);
						range_selector(ui, &mut self.options.range, Range::Nm20);
						range_selector(ui, &mut self.options.range, Range::Nm40);
						range_selector(ui, &mut self.options.range, Range::Nm80);
						range_selector(ui, &mut self.options.range, Range::Nm160);
						range_selector(ui, &mut self.options.range, Range::Nm320);
						range_selector(ui, &mut self.options.range, Range::Nm640);
					});
			});

			ui.horizontal(|ui| {
				ui.label("Heading");
				ui.add(
					DragValue::new(&mut self.options.heading)
						.clamp_range(0.0..=360.0)
						.speed(1.0),
				);
			});

			ui.horizontal(|ui| {
				ui.label("Azimuth");
				ui.add(
					DragValue::new(&mut self.options.sun_azimuth)
						.clamp_range(0.0..=360.0)
						.speed(1.0),
				);
				ui.label("Elevation");
				ui.add(
					DragValue::new(&mut self.options.sun_elevation)
						.clamp_range(0.0..=90.0)
						.speed(1.0),
				);
			});

			ui.horizontal(|ui| {
				ui.label("Aircraft Altitude");
				ui.add(
					DragValue::new(&mut self.options.altitude)
						.clamp_range(0.0..=50000.0)
						.speed(100.0),
				);
			});
		});

		tracy::zone!("Map Render");
		if let Some(renderer) = self.renderer.as_mut() {
			renderer.render(&self.options, device, queue, view, encoder);
		}
	}
}
