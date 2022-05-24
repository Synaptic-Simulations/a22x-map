use std::path::PathBuf;

use clap::Args;
use geo::{GeoTile, TileMetadata, FORMAT_VERSION};

use crate::{common::for_each_file, tiff::GeoTiff};

#[derive(Args)]
pub struct Generate {
	input: PathBuf,
	#[clap(short = 'o', long = "out", default_value = "output")]
	output: PathBuf,
	#[clap(short = 'r', long = "res", default_value_t = 1024)]
	resolution: u16,
	#[clap(short = 's', long = "hres", default_value_t = 50)]
	height_resolution: u16,
}

pub fn generate(generate: Generate) {
	let metadata = TileMetadata {
		version: FORMAT_VERSION,
		resolution: generate.resolution,
		height_resolution: generate.height_resolution,
	};
	match std::fs::create_dir_all(&generate.output) {
		Ok(_) => {},
		Err(e) => {
			eprintln!("{}", e);
			return;
		},
	}

	for_each_file(&generate.input, |entry| {
		let path = entry.path();
		let data = std::fs::read(&path)?;
		let tiff = GeoTiff::parse(&data)?;
		let tiff = &tiff;

		let tile = GeoTile::new(
			&metadata,
			(0..generate.resolution)
				.map(|x| (x, 0..generate.resolution))
				.flat_map(|(x, y)| {
					y.map(move |y| {
						let x = x as f32 / generate.resolution as f32;
						let y = y as f32 / generate.resolution as f32;
						tiff.sample(x, y).round() as _
					})
				}),
		)?;

		let file_name = path.file_name().unwrap().to_string_lossy().to_lowercase();
		let file_name = &file_name[10..];
		let (lat, lon) = match file_name.find(['n', 's']) {
			Some(x) => {
				let is_north = file_name.as_bytes()[x] == b'n';
				let remaining = &file_name[x + 1..];
				match remaining.find(['e', 'w']) {
					Some(y) => {
						let is_east = remaining.as_bytes()[y] == b'e';
						let lat = remaining[..y].parse::<i16>().unwrap();
						let lon = remaining[y + 1..y + 4].parse::<i16>().unwrap();
						(if is_north { lat } else { -lat }, if is_east { lon } else { -lon })
					},
					None => {
						eprintln!("unknown longitude: {}", path.display());
						return Ok(());
					},
				}
			},
			None => {
				eprintln!("unknown latitude: {}", path.display());
				return Ok(());
			},
		};

		tile.write_to_directory(&generate.output, lat, lon)?;

		Ok(())
	});

	match metadata.write_to_directory(&generate.output) {
		Ok(_) => {},
		Err(e) => {
			eprintln!("{}", e);
		},
	}
}
