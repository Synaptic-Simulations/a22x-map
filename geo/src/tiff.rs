use std::io::Cursor;

use tiff::{
	decoder::{Decoder, DecodingResult},
	TiffResult,
};

/// A GeoTIFF file.
pub struct GeoTiff {
	data: Vec<f32>,
	width: u32,
	height: u32,
}

impl GeoTiff {
	/// Parse a GeoTIFF file.
	/// Note that this is only tested against OpenTopography's data, and may not work for anything else.
	pub fn parse(data: &[u8]) -> TiffResult<Self> {
		let mut d = Decoder::new(Cursor::new(data))?;
		let data = d.read_image()?;
		let data = match data {
			DecodingResult::U8(data) => data.into_iter().map(|x| x as f32).collect(),
			DecodingResult::U16(data) => data.into_iter().map(|x| x as f32).collect(),
			DecodingResult::U32(data) => data.into_iter().map(|x| x as f32).collect(),
			DecodingResult::U64(data) => data.into_iter().map(|x| x as f32).collect(),
			DecodingResult::I8(data) => data.into_iter().map(|x| x as f32).collect(),
			DecodingResult::I16(data) => data.into_iter().map(|x| x as f32).collect(),
			DecodingResult::I32(data) => data.into_iter().map(|x| x as f32).collect(),
			DecodingResult::I64(data) => data.into_iter().map(|x| x as f32).collect(),
			DecodingResult::F32(data) => data,
			DecodingResult::F64(data) => data.into_iter().map(|x| x as f32).collect(),
		};

		let (width, height) = d.dimensions()?;

		Ok(GeoTiff { data, width, height })
	}
}
