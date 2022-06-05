use std::path::PathBuf;

use clap::Args;
use gdal::errors::GdalError;
use geo::{Dataset, TileMetadata, FORMAT_VERSION};

use crate::source::Raster;

#[derive(Args)]
pub struct Generate {
	input: PathBuf,
	#[clap(short = 'o', long = "out", default_value = "output.geo")]
	output: PathBuf,
	#[clap(short = 'r', long = "res", default_value_t = 1024)]
	resolution: u16,
	#[clap(short = 's', long = "hres", default_value_t = 50)]
	height_resolution: u16,
}

pub fn generate(generate: Generate) {
	let builder = Dataset::builder(TileMetadata {
		version: FORMAT_VERSION,
		resolution: generate.resolution,
		height_resolution: generate.height_resolution,
	});

	match (|| -> Result<(), GdalError> {
		let raster = Raster::load(&generate.input)?;
		let (lat, lon) = raster.get_pos()?;

		builder.add_tile(
			lat,
			lon,
			raster.get_data((generate.resolution as _, generate.resolution as _))?,
		);

		Ok(())
	})() {
		Ok(_) => {},
		Err(err) => {
			eprintln!("{}", err);
			return;
		},
	};

	match builder.finish(&generate.output) {
		Ok(_) => (),
		Err(err) => eprintln!("{}", err),
	}
}
