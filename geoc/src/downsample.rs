use std::path::PathBuf;

use clap::Args;
use geo::{GeoTile, TileMetadata};

use crate::common::for_each_file;

#[derive(Args)]
pub struct Downsample {
	input: PathBuf,
	#[clap(short = 'o', long = "out", default_value = "output")]
	output: PathBuf,
	#[clap(short = 'f', long = "factor", default_value_t = 2)]
	factor: u8,
}

pub fn downsample(downsample: Downsample) {
	match std::fs::create_dir_all(&downsample.output) {
		Ok(_) => {},
		Err(e) => {
			eprintln!("{}", e);
			return;
		},
	}

	let mut metadata = match TileMetadata::load_from_directory(&downsample.input) {
		Ok(metadata) => metadata,
		Err(err) => {
			eprintln!("metadata could not be loaded: {}", err);
			return;
		},
	};
	if metadata.resolution % downsample.factor as u16 != 0 {
		eprintln!("Downsample factor is not a factor of the resolution");
		return;
	}
	let resolution = metadata.resolution / downsample.factor as u16;
	if (resolution * resolution) % 256 != 0 {
		eprintln!("Final resolution is not a multiple of 256");
		return;
	}

	let new_res = metadata.resolution as usize / downsample.factor as usize;
	for_each_file(&downsample.input, |entry| {
		let path = entry.path();
		let tile = GeoTile::load(&metadata, &path)?;
		let data = tile.expand(&metadata);
		let mut downsampled = Vec::with_capacity(new_res * new_res);
		for i in 0..new_res {
			for j in 0..new_res {
				let mut sum = 0;
				for k in 0..downsample.factor as usize {
					for l in 0..downsample.factor as usize {
						sum += data[(i * downsample.factor as usize + k) * metadata.resolution as usize
							+ (j * downsample.factor as usize + l)];
					}
				}
				let value = sum as f32 / (downsample.factor * downsample.factor) as f32;
				downsampled.push(value.round() as _);
			}
		}
		let meta = TileMetadata {
			resolution: new_res as _,
			..metadata
		};
		let tile = GeoTile::new(&meta, downsampled)?;
		let (lat, lon) = GeoTile::get_coordinates_from_file_name(&path);
		tile.write_to_directory(&downsample.output, lat, lon)?;

		Ok(())
	});

	metadata.resolution = new_res as _;
	match metadata.write_to_directory(&downsample.output) {
		Ok(_) => {},
		Err(err) => {
			eprintln!("metadata could not be written: {}", err);
			return;
		},
	}
}
