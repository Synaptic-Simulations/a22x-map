use std::{
	error::Error,
	path::{Path, PathBuf},
	sync::{
		atomic::{AtomicBool, AtomicUsize, Ordering},
		Arc,
	},
};

use geo::{map_index_to_lat_lon, Dataset, DatasetBuilder, TileMetadata};
use rayon::prelude::*;

pub fn for_tile_in_output(
	output: PathBuf, metadata: TileMetadata,
	exec: impl Fn(i16, i16, &DatasetBuilder) -> Result<(), Box<dyn Error>> + Sync,
) {
	let was_quit = Arc::new(AtomicBool::new(false));
	let handler_used = was_quit.clone();
	let _ = ctrlc::set_handler(move || {
		if handler_used.load(Ordering::Acquire) {
			std::process::exit(1);
		}

		println!("\nExiting");
		handler_used.store(true, Ordering::Release);
	});

	fn make_builder(path: &Path, metadata: TileMetadata) -> DatasetBuilder {
		if path.exists() {
			if let Ok(x) = Dataset::load(path) {
				if metadata == x.metadata() {
					println!("Continuing from last execution");
					return DatasetBuilder::from_dataset(x);
				}
			}
		}
		DatasetBuilder::new(metadata)
	}

	let builder = make_builder(&output, metadata);

	let tiles = 360 * 180;
	let counter = AtomicUsize::new(1);
	let had_error = AtomicBool::new(false);

	print!("\r{}/{}", counter.load(Ordering::Relaxed), tiles);
	(0..180 * 360).into_par_iter().for_each(|index| {
		if had_error.load(Ordering::Acquire) || was_quit.load(Ordering::Acquire) {
			return;
		}

		let (lat, lon) = map_index_to_lat_lon(index);
		if !builder.tile_exists(lat, lon) {
			match exec(lat, lon, &builder) {
				Ok(_) => {},
				Err(e) => {
					println!("Error in tile {}, {}: {}", lat, lon, e);
					had_error.store(true, Ordering::Release);
				},
			}
		}

		print!("\r{}/{}", counter.fetch_add(1, Ordering::Relaxed), tiles);
	});

	(!had_error.load(Ordering::Relaxed))
		.then(|| builder.finish(&output))
		.map(|x| match x {
			Ok(_) => {},
			Err(e) => println!("Error saving output: {}", e),
		});
}
