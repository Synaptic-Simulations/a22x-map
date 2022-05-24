static RANGE_TO_RADIANS: &[f32] = &[
	0.00010644806,
	0.00021289594,
	0.00031934388,
	0.0006467901,
	0.0012935802,
	0.00323395052,
	0.00646790104,
	0.01293580208,
	0.02587160417,
	0.05174320834,
	0.10348641668,
	0.21278667699,
	0.42557335399,
];

#[derive(Copy, Clone, Debug)]
pub enum Range {
	F1000,
	F2000,
	F3000,
	Nm1,
	Nm2,
	Nm5,
	Nm10,
	Nm20,
	Nm40,
	Nm80,
	Nm160,
	Nm320,
	Nm640,
}

#[derive(Copy, Clone, Debug)]
struct LatLon {
	lat: f32,
	lon: f32,
}

fn range_to_radians(range: Range) -> f32 { RANGE_TO_RADIANS[range as usize] }

fn project(x: f32, y: f32, center: LatLon, range: Range) -> LatLon {
	let scale = range_to_radians(range);
	let x = (x - 0.5) * scale;
	let y = (y - 0.5) * scale;

	let c = (x * x + y * y).sqrt();
	let latsin = center.lat.sin();
	let latcos = center.lat.cos();
	let csin = c.sin();
	let ccos = c.cos();
	let lat = (ccos * latsin + y * csin * latcos / c).asin();
	let lon = center.lon + (x * csin).atan2(c * latcos * ccos - y * latsin * csin);

	LatLon { lat, lon }
}
