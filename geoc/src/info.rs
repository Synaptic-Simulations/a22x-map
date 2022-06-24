use std::{
	fmt::Display,
	path::PathBuf,
	sync::atomic::{AtomicUsize, Ordering},
};

use clap::Args;
use geo::{map_index_to_lat_lon, Dataset};
use rayon::prelude::*;

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

	let water_tiles = AtomicUsize::new(0);
	let water_tiles_size = AtomicUsize::new(0);
	let tiles = 360 * 180;
	let counter = AtomicUsize::new(1);
	(0..tiles).into_par_iter().for_each(|index| {
		let (lat, lon) = map_index_to_lat_lon(index);
		match dataset.get_tile_and_compressed_size(lat, lon) {
			Some(Ok((tile, size))) => {
				if tile.into_iter().all(|h| h == -500) {
					water_tiles.fetch_add(1, Ordering::Relaxed);
					water_tiles_size.fetch_add(size, Ordering::Relaxed);
				}
			},
			Some(Err(e)) => {
				eprintln!("{}", e);
			},
			None => {},
		}

		print!("\r{}/{}", counter.fetch_add(1, Ordering::Relaxed), tiles);
	});
	print!("\r");
	println!("  Water tiles: {}", water_tiles.load(Ordering::Relaxed));
	println!(
		"  Water tiles size: {}",
		Size(water_tiles_size.load(Ordering::Relaxed) as _)
	);
}
