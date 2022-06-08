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
	#[clap(short = 'w', long = "water")]
	water: PathBuf,
	#[clap(short = 'o', long = "out", default_value = "output.geo")]
	output: PathBuf,
	#[clap(short = 'r', long = "res", default_value_t = 1024)]
	resolution: u16,
	#[clap(short = 's', long = "hres", default_value_t = 50)]
	height_resolution: u16,
	#[clap(short = 'c', long = "compression", default_value_t = 21)]
	compression_level: i8,
}

pub fn generate(generate: Generate) {
	let source = match Raster::load(&generate.input) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("Error loading data source: {:?}", err);
			return;
		},
	};
	let water = match Raster::load(&generate.water) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("Error loading water source: {:?}", err);
			return;
		},
	};
	let metadata = TileMetadata {
		version: FORMAT_VERSION,
		resolution: generate.resolution,
		height_resolution: generate.height_resolution,
	};

	for_tile_in_output(
		generate.output,
		generate.compression_level,
		metadata,
		|lat, lon, builder| {
			let bottom_left = LatLon {
				lat: lat as f64,
				lon: lon as f64,
			};
			let top_right = LatLon {
				lat: (lat + 1) as f64,
				lon: (lon + 1) as f64,
			};

			source
				.get_data(bottom_left, top_right, metadata.resolution as _)
				.and_then(|data| {
					water
						.get_data(bottom_left, top_right, metadata.resolution as _)
						.map(|water: Vec<u8>| (data, water))
				})
				.map(|(mut data, water)| {
					for (height, &mask) in data.iter_mut().zip(water.iter()) {
						if mask != 0 {
							*height = -500;
						}
					}
					builder.add_tile(lat, lon, data);
				});

			Ok(())
		},
	);
}
