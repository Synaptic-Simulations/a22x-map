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

static TILES_FOR_RANGE: &[i16] = &[1, 1, 1, 1, 1, 2, 4, 9, 20];

static LOD_FOR_RANGE: &[usize] = &[0, 0, 0, 0, 0, 1, 1, 2, 2];

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

pub fn range_to_radians(range: Range) -> f32 { RANGE_TO_RADIANS[range as usize] }

pub fn tiles_for_range(range: Range) -> i16 { TILES_FOR_RANGE[range as usize] }

pub fn lod_for_range(range: Range) -> usize { LOD_FOR_RANGE[range as usize] }
