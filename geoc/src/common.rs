use std::{
	error::Error,
	io::Write,
	path::Path,
	sync::{
		atomic::{AtomicBool, AtomicUsize, Ordering},
		Arc,
	},
	time::Duration,
};

use geo::{map_index_to_lat_lon, Dataset, DatasetBuilder, TileMetadata};
use rayon::prelude::*;

pub fn for_tile_in_output(
	output: &Path, metadata: TileMetadata,
	exec: impl Fn(i16, i16, &DatasetBuilder) -> Result<(), Box<dyn Error>> + Sync,
) {
	let was_quit = Arc::new(AtomicBool::new(false));
	let handler_used = was_quit.clone();
	let was_quit = &was_quit;

	let _ = ctrlc::set_handler(move || {
		if handler_used.load(Ordering::Acquire) {
			std::process::exit(1);
		}

		println!("\nFinishing up, press Ctrl + C again to exit immediately (will result in some data loss)");
		handler_used.store(true, Ordering::Release);
	});

	fn make_builder(path: &Path, metadata: TileMetadata) -> Result<DatasetBuilder, std::io::Error> {
		if let Ok(x) = Dataset::load(path) {
			if metadata == x.metadata() {
				println!("Continuing from last execution");
				return DatasetBuilder::from_dataset(&path, x);
			}
		}
		DatasetBuilder::new(&path, metadata)
	}

	let builder = match make_builder(&output, metadata) {
		Ok(x) => x,
		Err(e) => {
			eprintln!("{}", e);
			return;
		},
	};
	let rbuilder = &builder;

	let tiles = 360 * 180;
	let counter = AtomicUsize::new(1);
	let had_error = AtomicBool::new(false);
	let had_error = &had_error;

	let _ = crossbeam::scope(move |scope| {
		scope.spawn(move |_| {
			while !was_quit.load(Ordering::Acquire) {
				std::thread::sleep(Duration::from_secs(10));
				let _ = rbuilder.flush();
			}
		});

		print!("\r{}/{}", counter.load(Ordering::Relaxed), tiles);
		(0..tiles).into_par_iter().for_each(|index| {
			tracy::zone!("Process tile");
			if was_quit.load(Ordering::Acquire) {
				return;
			}

			let (lat, lon) = map_index_to_lat_lon(index);
			if !rbuilder.tile_exists(lat, lon) {
				match exec(lat, lon, &rbuilder) {
					Ok(_) => {},
					Err(e) => {
						println!("\nError in tile {}, {}: {}", lat, lon, e);
						had_error.store(true, Ordering::Release);
					},
				}
			}

			print!("\r{}/{}", counter.fetch_add(1, Ordering::Relaxed), tiles);
			let _ = std::io::stdout().flush();
		});

		was_quit.store(true, Ordering::Release);
	});

	(!had_error.load(Ordering::Relaxed))
		.then(|| builder.finish())
		.map(|x| match x {
			Ok(_) => {},
			Err(e) => println!("Error saving output: {}", e),
		});
}
