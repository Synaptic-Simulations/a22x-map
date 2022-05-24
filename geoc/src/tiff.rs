use std::io::Cursor;

use tiff::{
	decoder::{Decoder, DecodingResult},
	TiffResult,
};

/// A GeoTIFF file.
pub struct GeoTiff {
	data: Vec<i16>,
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
			DecodingResult::U8(data) => data.into_iter().map(|x| x as i16).collect(),
			DecodingResult::U16(data) => data.into_iter().map(|x| x as i16).collect(),
			DecodingResult::U32(data) => data.into_iter().map(|x| x as i16).collect(),
			DecodingResult::U64(data) => data.into_iter().map(|x| x as i16).collect(),
			DecodingResult::I8(data) => data.into_iter().map(|x| x as i16).collect(),
			DecodingResult::I16(data) => data,
			DecodingResult::I32(data) => data.into_iter().map(|x| x as i16).collect(),
			DecodingResult::I64(data) => data.into_iter().map(|x| x as i16).collect(),
			DecodingResult::F32(data) => data.into_iter().map(|x| x as i16).collect(),
			DecodingResult::F64(data) => data.into_iter().map(|x| x as i16).collect(),
		};

		let (width, height) = d.dimensions()?;

		Ok(GeoTiff { data, width, height })
	}

	/// Bilinearly sample the GeoTIFF file at the given UV coordinates.
	pub fn sample(&self, x: f32, y: f32) -> f32 {
		fn lerp(from: f32, to: f32, t: f32) -> f32 { from + (to - from) * t }

		let x_pixel = x * self.width as f32;
		let y_pixel = y * self.height as f32;
		let x_low = x_pixel.floor();
		let y_low = y_pixel.floor();
		let x_delta = x_pixel - x_low;
		let y_delta = y_pixel - y_low;
		let x_low = x_low as u32;
		let y_low = y_low as u32;
		let x_high = x_low + 1;
		let y_high = y_low + 1;

		let xlyl = self.data[(y_low * self.width + x_low) as usize] as f32;
		let xhyl = self.data[(y_low * self.width + x_high) as usize] as f32;
		let xlyh = self.data[(y_high * self.width + x_low) as usize] as f32;
		let xhyh = self.data[(y_high * self.width + x_high) as usize] as f32;

		let yl = lerp(xlyl, xhyl, x_delta);
		let yh = lerp(xlyh, xhyh, x_delta);
		lerp(yl, yh, y_delta)
	}

	/// Downsample the GeoTIFF file to a 512x512 grid with altitudes in 100 meter intervals,
	/// with 0 being an altitude of -500 meter MSL.
	///
	/// To map the values back, use `(value - 5) * 100`.
	pub fn downsample(self) -> Vec<i16> {
		let mut result = Vec::with_capacity(512 * 512);

		for x in 0..512 {
			for y in 0..512 {
				let x = x as f32 / 512.0;
				let y = y as f32 / 512.0;

				result.push(self.sample(x, y).round() as _);
			}
		}

		result
	}
}
