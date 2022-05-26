use egui::{ComboBox, Context, DragValue, Window};
use render::{
	range::{Mode, Range},
	LatLon,
	Renderer,
};
use wgpu::{Device, Queue, RenderPass, TextureFormat};

pub struct Ui {
	data_path: String,
	position: LatLon,
	range: Range,
	renderer: Option<Renderer>,
}

impl Ui {
	pub fn new() -> Self {
		Self {
			data_path: String::new(),
			position: LatLon { lat: 0., lon: 0. },
			range: Range::Nm10,
			renderer: None,
		}
	}

	pub fn update<'a>(
		&'a mut self, ctx: &Context, pass: &mut RenderPass<'a>, device: &Device, queue: &Queue, format: TextureFormat,
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
								queue,
								format,
								data,
								self.position,
								self.range,
								Mode::FullPage,
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
					.selected_text(range_to_str(self.range))
					.show_ui(ui, |ui| {
						ui.selectable_value(&mut self.range, Range::Nm2, range_to_str(Range::Nm2));
						ui.selectable_value(&mut self.range, Range::Nm5, range_to_str(Range::Nm5));
						ui.selectable_value(&mut self.range, Range::Nm10, range_to_str(Range::Nm10));
						ui.selectable_value(&mut self.range, Range::Nm20, range_to_str(Range::Nm20));
						ui.selectable_value(&mut self.range, Range::Nm40, range_to_str(Range::Nm40));
						ui.selectable_value(&mut self.range, Range::Nm80, range_to_str(Range::Nm80));
						ui.selectable_value(&mut self.range, Range::Nm160, range_to_str(Range::Nm160));
						ui.selectable_value(&mut self.range, Range::Nm320, range_to_str(Range::Nm320));
						ui.selectable_value(&mut self.range, Range::Nm640, range_to_str(Range::Nm640));
					});
			});
		});

		tracy::zone!("Map Render");
		if let Some(renderer) = self.renderer.as_mut() {
			renderer.render(pass);
		}
	}
}

fn range_to_str(range: Range) -> &'static str {
	match range {
		Range::Nm2 => "2 nm",
		Range::Nm5 => "5 nm",
		Range::Nm10 => "10 nm",
		Range::Nm20 => "20 nm",
		Range::Nm40 => "40 nm",
		Range::Nm80 => "80 nm",
		Range::Nm160 => "160 nm",
		Range::Nm320 => "320 nm",
		Range::Nm640 => "640 nm",
	}
}
