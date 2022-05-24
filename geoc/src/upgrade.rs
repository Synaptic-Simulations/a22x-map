use std::path::PathBuf;

use clap::Args;
use geo::{GeoTile, TileMetadata, FORMAT_VERSION};

use crate::common::for_each_file;

#[derive(Args)]
pub struct Upgrade {
	input: PathBuf,
	#[clap(short = 'o', long = "out", default_value = "output")]
	output: PathBuf,
}

pub fn upgrade(upgrade: Upgrade) {
	match std::fs::create_dir_all(&upgrade.output) {
		Ok(_) => {},
		Err(e) => {
			eprintln!("{}", e);
			return;
		},
	}

	let mut metadata = match TileMetadata::load_from_directory(&upgrade.input) {
		Ok(metadata) => metadata,
		Err(err) => {
			eprintln!("metadata could not be loaded: {}", err);
			return;
		},
	};
	if metadata.version == FORMAT_VERSION {
		eprintln!("already up to date");
		return;
	}

	for_each_file(&upgrade.input, |entry| {
		let path = entry.path();

		let (tile, lat, lon) = GeoTile::load(&metadata, &path)?;
		tile.write_to_directory(&upgrade.output, lat, lon)?;

		Ok(())
	});

	metadata.version = FORMAT_VERSION;
	match metadata.write_to_directory(&upgrade.output) {
		Ok(_) => {},
		Err(err) => {
			eprintln!("metadata could not be written: {}", err);
			return;
		},
	}
}
