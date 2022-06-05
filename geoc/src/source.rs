use std::path::Path;

use gdal::{
	errors::GdalError,
	raster::{GdalType, ResampleAlg},
	Dataset,
};

pub struct Raster {
	set: Dataset,
}

impl Raster {
	pub fn load(path: &Path) -> Result<Self, GdalError> { Dataset::open(path).map(|set| Self { set }) }

	pub fn get_pos(&self) -> Result<(i16, i16), GdalError> {
		println!("{}", self.set.layer_count());

		let transform = self.set.geo_transform()?;
		Ok((transform[3] as i16, transform[0] as i16))
	}

	pub fn get_data<T: GdalType + Copy>(&self, res: (usize, usize)) -> Result<Vec<T>, GdalError> {
		self.set
			.rasterband(1)?
			.read_as((0, 0), self.set.raster_size(), res, Some(ResampleAlg::Lanczos))
			.map(|data| data.data)
	}
}
