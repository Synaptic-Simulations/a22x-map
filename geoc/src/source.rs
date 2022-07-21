use std::path::{Path, PathBuf};

use gdal::{
	errors::GdalError,
	raster::{GdalType, ResampleAlg},
	Dataset,
};
use thread_local::ThreadLocal;

#[derive(Copy, Clone)]
pub struct LatLon {
	pub lat: f64,
	pub lon: f64,
}

struct Transform([f64; 6]);

impl Transform {
	fn to_geo(&self, x: f64, y: f64) -> LatLon {
		LatLon {
			lat: self.0[3] + y * self.0[5],
			lon: self.0[0] + x * self.0[1],
		}
	}

	fn to_image(&self, pos: LatLon) -> (f64, f64) {
		((pos.lon - self.0[0]) / self.0[1], (pos.lat - self.0[3]) / self.0[5])
	}
}

pub struct Raster {
	path: PathBuf,
	set: ThreadLocal<Dataset>,
	transform: Transform,
}

impl Raster {
	pub fn load(path: &Path) -> Result<Self, GdalError> {
		tracy::zone!("Load raster");

		let dataset = Dataset::open(path)?;
		let transform = dataset.geo_transform()?;

		assert_eq!(transform[2], 0.0, "row rotation must be 0");
		assert_eq!(transform[4], 0.0, "column rotation must be 0");
		assert!(transform[5] <= 0.0, "y scale must be negative");

		let set = ThreadLocal::new();
		set.get_or(|| dataset);

		Ok(Self {
			path: path.to_path_buf(),
			set,
			transform: Transform(transform),
		})
	}

	pub fn get_data<T: GdalType + Copy>(&self, bottom_left: LatLon, top_right: LatLon, res: usize) -> Option<Vec<T>> {
		tracy::zone!("Get raster data");

		let set = self
			.set
			.get_or(|| Dataset::open(&self.path).expect("Failed to open dataset on thread"));

		let (xl, yb) = self.transform.to_image(bottom_left);
		let (xr, yt) = self.transform.to_image(top_right);
		let (xl, yt) = (xl.floor() as isize, yt.floor() as isize);
		let (xr, yb) = (xr.floor() as isize, yb.floor() as isize);
		let (w, h) = set.raster_size();

		if xl < 0 || yt < 0 || xr >= w as isize || yb >= h as isize {
			return None;
		}

		set.rasterband(1)
			.expect("Band with index 1 not present")
			.read_as(
				(xl, yt),
				((xr - xl) as usize, (yb - yt) as usize),
				(res, res),
				Some(ResampleAlg::Lanczos),
			)
			.ok()
			.map(|buf| buf.data)
	}

	pub fn get_data_for_hillshade<T: GdalType + Copy>(
		&self, bottom_left: LatLon, top_right: LatLon, res: usize,
	) -> Option<(Vec<T>, bool)> {
		tracy::zone!("Get raster data");

		let set = self
			.set
			.get_or(|| Dataset::open(&self.path).expect("Failed to open dataset on thread"));

		let (xl, yb) = self.transform.to_image(bottom_left);
		let (xr, yt) = self.transform.to_image(top_right);
		let (xl, yt) = (xl.floor() as isize, yt.floor() as isize);
		let (xr, yb) = (xr.floor() as isize, yb.floor() as isize);
		let (w, h) = set.raster_size();

		if xl < 0 || yt < 0 || xr >= w as isize || yb >= h as isize {
			return None;
		}

		let (left_wrap, top_wrap, right_wrap, bottom_wrap) =
			(xl == 0, yt == 0, xr == w as isize - 1, yb == h as isize - 1);

		if left_wrap || top_wrap || right_wrap || bottom_wrap {
			set.rasterband(1)
				.expect("Band with index 1 not present")
				.read_as(
					(xl, yt),
					((xr - xl) as usize, (yb - yt) as usize),
					(res, res),
					Some(ResampleAlg::Lanczos),
				)
				.ok()
				.map(|b| (b.data, false))
		} else {
			set.rasterband(1)
				.expect("Band with index 1 not present")
				.read_as(
					(xl - 1, yt - 1),
					((xr - xl) as usize + 2, (yb - yt) as usize + 2),
					(res + 2, res + 2),
					Some(ResampleAlg::Lanczos),
				)
				.ok()
				.map(|b| (b.data, true))
		}
	}
}
