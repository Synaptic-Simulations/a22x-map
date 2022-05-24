use std::path::PathBuf;

use clap::Args;
use geo::TileMetadata;

#[derive(Args)]
pub struct Metadata {
	input: PathBuf,
}

pub fn metadata(metadata: Metadata) {
	let metadata = match TileMetadata::load_from_directory(&metadata.input) {
		Ok(metadata) => metadata,
		Err(err) => {
			eprintln!("metadata could not be loaded: {}", err);
			return;
		},
	};

	println!("version: {}", metadata.version);
	println!("resolution: {}", metadata.resolution);
	println!("height resolution: {}", metadata.height_resolution);
}
