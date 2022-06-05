use std::path::PathBuf;

use clap::Args;
use geo::Dataset;

#[derive(Args)]
pub struct Metadata {
	input: PathBuf,
}

pub fn metadata(metadata: Metadata) {
	let metadata = match Dataset::load(metadata.input) {
		Ok(x) => x.metadata(),
		Err(err) => {
			eprintln!("dataset could not be loaded: {}", err);
			return;
		},
	};

	println!("version: {}", metadata.version);
	println!("resolution: {}", metadata.resolution);
	println!("height resolution: {}", metadata.height_resolution);
}
