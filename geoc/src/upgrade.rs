use std::{
	path::PathBuf,
	sync::atomic::{AtomicUsize, Ordering},
};

use clap::Args;
use geo::{Dataset, TileMetadata, FORMAT_VERSION};
use rayon::prelude::*;

#[derive(Args)]
pub struct Upgrade {
	input: PathBuf,
	#[clap(short = 'o', long = "out", default_value = "output")]
	output: PathBuf,
}

pub fn upgrade(upgrade: Upgrade) {
	let source = match Dataset::load(upgrade.input) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("{}", err);
			return;
		},
	};

	if source.metadata().version == FORMAT_VERSION {
		eprintln!("already up to date");
		return;
	}

	let dest = Dataset::builder(TileMetadata {
		version: FORMAT_VERSION,
		..source.metadata()
	});

	let max = 180 * 360;
	let counter = AtomicUsize::new(1);

	(-90..90).into_par_iter().for_each(|lat| {
		(-180..180).into_par_iter().for_each(|lon| {
			source.get_tile(lat, lon).map(|data| dest.add_tile(lat, lon, data));

			print!("\r{}/{}", counter.fetch_add(1, Ordering::SeqCst), max);
		});
	});

	if let Err(e) = dest.finish(&upgrade.output) {
		eprintln!("{}", e);
	}
}
