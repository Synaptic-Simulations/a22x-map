use std::path::PathBuf;

use clap::Args;
use geo::{TileMetadata, FORMAT_VERSION};

use crate::{
	common::for_tile_in_output,
	source::{LatLon, Raster},
};

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
	let source = match Raster::load(&generate.input) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("Error loading data source: {:?}", err);
			return;
		},
	};
	let metadata = TileMetadata {
		version: FORMAT_VERSION,
		resolution: generate.resolution,
		height_resolution: generate.height_resolution,
	};

	for_tile_in_output(generate.output, metadata, |lat, lon, builder| {
		source
			.get_data(
				LatLon {
					lat: lat as f64,
					lon: lon as f64,
				},
				LatLon {
					lat: (lat + 1) as f64,
					lon: (lon + 1) as f64,
				},
				metadata.resolution as _,
			)
			.map(|data| builder.add_tile(lat, lon, data));

		Ok(())
	});
}
