use std::path::PathBuf;

use clap::Args;
use geo::{Dataset, HillshadeMetadata, HILLSHADE_FORMAT_VERSION};

use crate::common::for_tile_in_hillshade_output;

#[derive(Args)]
/// Generate a hillshade dataset from a dataset.
pub struct Hillshade {
	input: PathBuf,
	#[clap(short = 'o', long = "out", default_value = "output.geo")]
	output: PathBuf,
	#[clap(short = 'c', long = "compression", default_value_t = 21)]
	compression_level: i8,
}

pub fn hillshade(hillshade: Hillshade) {
	let source = match Dataset::load(&hillshade.input) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("Error loading data source: {:?}", err);
			return;
		},
	};

	let source_metadata = source.metadata();
	let metadata = HillshadeMetadata {
		version: HILLSHADE_FORMAT_VERSION,
		resolution: source_metadata.resolution,
		tiling: source_metadata.tiling,
	};

	for_tile_in_hillshade_output(
		&hillshade.output,
		hillshade.compression_level,
		metadata,
		|lat, lon, builder| {
			if let Some(source) = source.get_tile(lat, lon).transpose()? {
				let mut out = vec![0; source.len()];

				builder.add_tile(lat, lon, out)?;
			}

			Ok(())
		},
	);
}
