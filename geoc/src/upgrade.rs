use std::path::PathBuf;

use clap::Args;
use geo::{Dataset, TileMetadata, FORMAT_VERSION};

use crate::common::for_tile_in_output;

#[derive(Args)]
/// Upgrade a dataset to the latest format version.
pub struct Upgrade {
	input: PathBuf,
	#[clap(short = 'o', long = "out", default_value = "output")]
	output: PathBuf,
	#[clap(short = 'd', long = "delta")]
	delta_compressed: bool,
}

pub fn upgrade(upgrade: Upgrade) {
	let source = match Dataset::load(&upgrade.input) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("{}", err);
			return;
		},
	};

	for_tile_in_output(
		&upgrade.output,
		TileMetadata {
			version: FORMAT_VERSION,
			..source.metadata()
		},
		|lat, lon, builder| {
			source
				.get_tile(lat, lon)
				.map(|data| builder.add_tile(lat, lon, data?))
				.transpose()?;
			Ok(())
		},
	);
}
