use std::{
	error::Error,
	path::PathBuf,
	sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::sync::Arc;

use geo::{Dataset, DatasetBuilder, TileMetadata};
use rayon::prelude::*;

pub fn for_tile_in_output(
	output: PathBuf, metadata: TileMetadata, exec: impl Fn(i16, i16, &DatasetBuilder) -> Result<(), Box<dyn Error>> + Sync,
) {
	let was_quit = Arc::new(AtomicBool::new(false));
	let handler_used = was_quit.clone();
    let _ = ctrlc::set_handler(move || {
        println!("Exiting");
		handler_used.store(true, Ordering::Release);
    });

	let builder = if output.exists() {
		if let Ok(x) = Dataset::load(&output) {
			if metadata == x.metadata() {
				println!("Continuing from last execution");
				DatasetBuilder::from_dataset(x)
			} else {
				DatasetBuilder::new(metadata)
			}
		} else {
			DatasetBuilder::new(metadata)
		}
	} else {
		DatasetBuilder::new(metadata)
	};

	let tiles = 360 * 180;
	let counter = AtomicUsize::new(1);
	let had_error = AtomicBool::new(false);

	(-90..90).into_par_iter().for_each(|lat| {
		(-180..180).into_par_iter().for_each(|lon| {
			if had_error || was_quit.load(Ordering::Acquire) {
				return;
			}

			match exec(lat, lon, &builder) {
				Ok(_) => {},
				Err(e) => {
					println!("Error in tile {}, {}: {}", lat, lon, e);
					had_error.store(true, Ordering::Release);
				},
			}

			print!("\r{}/{}", counter.fetch_add(1, Ordering::Relaxed), tiles);
		})
	});

	match (!had_error.load(Ordering::Relaxed)).then(|| builder.finish(&output)) {
		Ok(_) => {},
		Err(e) => println!("Error saving output: {}", e),
	}
}
