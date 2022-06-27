use std::{fmt::Display, path::PathBuf};

use clap::Args;
use geo::Dataset;

#[derive(Args)]
/// Give information about the dataset.
pub struct Info {
	input: PathBuf,
}

struct Size(usize);

impl Display for Size {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let size = self.0;
		if size < 1000 {
			write!(f, "{} B", size)
		} else if size < 1000 * 1000 {
			write!(f, "{:.2} KB", size as f64 / 1000.0)
		} else if size < 1000 * 1000 * 1000 {
			write!(f, "{:.2} MiB", size as f64 / 1000.0 / 1000.0)
		} else {
			write!(f, "{:.2} GiB", size as f64 / 1000.0 / 1000.0 / 1000.0)
		}
	}
}

pub fn info(info: Info) {
	let dataset = match Dataset::load(&info.input) {
		Ok(x) => x,
		Err(err) => {
			eprintln!("dataset could not be loaded: {}", err);
			return;
		},
	};
	let metadata = dataset.metadata();

	println!("Metadata");
	println!("  Version: {}", metadata.version);
	println!("  Resolution: {}", metadata.resolution);
	println!("  Height resolution: {}", metadata.height_resolution);

	println!();

	println!("Tiles");
	println!("  Tile count: {}", dataset.tile_count());
}
