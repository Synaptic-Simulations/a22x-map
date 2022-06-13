use std::path::PathBuf;

use clap::Args;
use geo::{map_lat_lon_to_index, Dataset, TileMetadata, FORMAT_VERSION};
use rand::prelude::*;
use sysinfo::{RefreshKind, SystemExt};

use crate::{common::for_tile_in_output, info::Size};

#[derive(Args)]
/// Optimize a dataset as a new dataset.
pub struct Optimize {
	input: PathBuf,
	#[clap(short = 'o', long = "output")]
	output: PathBuf,
}

pub fn optimize(optimize: Optimize) {
	let source = match Dataset::load(&optimize.input) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("Error loading data source: {:?}", err);
			return;
		},
	};

	let source_metadata = source.metadata();
	let metadata = TileMetadata {
		version: FORMAT_VERSION,
		..source_metadata
	};

	let mut r = SmallRng::from_entropy();
	let mem = 4 * 1024 * 1024 * 1024;
	let samples = mem / (source_metadata.resolution as usize * source_metadata.resolution as usize * 2);

	let mut dict = Vec::with_capacity(mem);
	let mut samples = Vec::with_capacity(samples);
	let mut tile_map = vec![0; 360 * 180];
	let mut counter = 0;

	loop {
		let lat = r.gen_range(-90..90);
		let lon = r.gen_range(-180..180);
		let index = map_lat_lon_to_index(lat, lon);
		if tile_map[index] == 2 {
			continue;
		}
		if samples.len() == samples.capacity() {
			break;
		}
		if counter > 360 * 180 * 2 {
			break;
		}

		if let Some(data) = source.get_raw_tile_data(lat, lon) {
			match data {
				Ok(data) => {
					if dict.len() + data.len() >= mem {
						break;
					}

					dict.extend_from_slice(&data);
					samples.push(data.len());
					tile_map[index] = 2;
				},
				Err(err) => {
					eprintln!("Error loading tile: {:?}", err);
					return;
				},
			}
		} else {
			tile_map[index] = 1;
		}

		counter += 1;
	}

	println!("Collected {} samples", samples.len());
	let dict = match geo::zstd::dict::from_continuous(&dict, &samples, 200 * 1024 * 1024) {
		Ok(dict) => dict,
		Err(err) => {
			eprintln!("Error creating dictionary: {:?}", err);
			return;
		},
	};

	for_tile_in_output(&optimize.output, 21, &dict, metadata, |lat, lon, builder| {
		if let Some(data) = source.get_tile(lat, lon).transpose()? {
			if !data.iter().copied().all(|x| x == -500) {
				builder.add_tile(lat, lon, data)?;
			}
		}

		Ok(())
	});
}
