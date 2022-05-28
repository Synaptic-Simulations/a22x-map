use std::fmt::Display;

static RANGE_TO_RADIANS: &[f32] = &[
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

pub(crate) static RANGE_TO_DEGREES: &[f32] = &[
	0.0741166859218728,
	0.1852917159505975,
	0.3705834319011951,
	0.7411668638023902,
	1.482333728177738,
	2.964667456355476,
	5.929334912710953,
	12.19177852817075,
	24.38355705691446,
];

pub(crate) static RANGES: &[Range] = &[
	Range::Nm2,
	Range::Nm5,
	Range::Nm10,
	Range::Nm20,
	Range::Nm40,
	Range::Nm80,
	Range::Nm160,
	Range::Nm320,
	Range::Nm640,
];

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Range {
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

impl Range {
	pub fn vertical_degrees(self) -> f32 { RANGE_TO_DEGREES[self as usize] }

	pub fn vertical_radians(self) -> f32 { RANGE_TO_RADIANS[self as usize] }

	pub fn vertical_tiles_loaded(self) -> u32 { self.vertical_degrees().ceil() as u32 + 1 }

	pub fn to_str(self) -> &'static str {
		match self {
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
}

impl Display for Range {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", self.to_str()) }
}
