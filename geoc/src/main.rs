use std::{
	fs::File,
	io::Cursor,
	path::PathBuf,
	sync::atomic::{AtomicUsize, Ordering},
};

use clap::Parser;
use geo::{GeoTile, TileMetadata, FORMAT_VERSION};
use rayon::prelude::*;

use crate::tiff::GeoTiff;

pub mod tiff;

#[derive(Parser)]
pub struct Options {
	input: PathBuf,
	#[clap(short = 'o', long = "out", default_value = "output")]
	output: PathBuf,
	#[clap(short = 'r', long = "res", default_value_t = 512)]
	resolution: u16,
	#[clap(short = 'h', long = "hres", default_value_t = 50)]
	height_resolution: u16,
}

fn amain() {
	let opts: Options = Options::parse();

	let dir = match std::fs::read_dir(&opts.input) {
		Ok(x) => x,
		Err(e) => {
			eprintln!("{}", e);
			std::process::exit(1);
		},
	};

	let metadata = TileMetadata {
		version: FORMAT_VERSION,
		resolution: opts.resolution,
		height_resolution: opts.height_resolution,
	};
	let num = std::fs::read_dir(&opts.input).unwrap().count();
	let done = AtomicUsize::new(0);

	dir.par_bridge().for_each(|entry| {
		let entry = match entry {
			Ok(x) => x,
			Err(e) => {
				eprintln!("{}", e);
				return;
			},
		};

		let path = entry.path();
		let data = match std::fs::read(&path) {
			Ok(x) => x,
			Err(e) => {
				eprintln!("{}", e);
				return;
			},
		};
		let tiff = match GeoTiff::parse(&data) {
			Ok(x) => x,
			Err(e) => {
				eprintln!("{}", e);
				return;
			},
		};
		let tiff = &tiff;

		let tile = GeoTile::new(
			&metadata,
			(0..opts.resolution)
				.map(|x| (x, 0..opts.resolution))
				.flat_map(|(x, y)| {
					y.map(move |y| {
						let x = x as f32 / opts.resolution as f32;
						let y = y as f32 / opts.resolution as f32;
						tiff.sample(x, y).round() as _
					})
				}),
		);
		let tile = match tile {
			Ok(x) => x,
			Err(e) => {
				eprintln!("{}", e);
				return;
			},
		};

		let file_name = path.file_name().unwrap().to_string_lossy().to_lowercase();
		let file_name = &file_name[10..];
		let (lat, lon) = match file_name.find(['n', 's']) {
			Some(x) => {
				let is_north = file_name.as_bytes()[x] == b'n';
				let remaining = &file_name[x + 1..];
				match remaining.find(['e', 'w']) {
					Some(y) => {
						let is_east = remaining.as_bytes()[y] == b'e';
						let lat = remaining[..y].parse::<i16>().unwrap();
						let lon = remaining[y + 1..y + 4].parse::<i16>().unwrap();
						(if is_north { lat } else { -lat }, if is_east { lon } else { -lon })
					},
					None => {
						eprintln!("unknown longitude: {}", path.display());
						return;
					},
				}
			},
			None => {
				eprintln!("unknown latitude: {}", path.display());
				return;
			},
		};

		match tile.write_to_directory(&opts.output, lat, lon) {
			Ok(_) => {},
			Err(e) => {
				eprintln!("{}", e);
				return;
			},
		};

		done.fetch_add(1, Ordering::Acquire);
		println!("{}/{}", done.load(Ordering::Relaxed), num);
	});
}
