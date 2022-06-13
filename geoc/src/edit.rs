use std::{cell::RefCell, path::PathBuf};

use clap::Args;
use geo::{Dataset, TileMetadata, FORMAT_VERSION};
use resize::{Pixel::Gray16, Resizer, Type};
use rgb::FromSlice;
use thread_local::ThreadLocal;

use crate::common::for_tile_in_output;

#[derive(Args)]
/// Create a new dataset derived from another.
pub struct Edit {
	input: PathBuf,
	#[clap(short = 'o', long = "output")]
	output: PathBuf,
	#[clap(short = 'r', long = "res", default_value_t = 1024)]
	resolution: u16,
	#[clap(short = 's', long = "hres", default_value_t = 50)]
	height_resolution: u16,
	#[clap(short = 'c', long = "compression", default_value_t = 21)]
	compression_level: i8,
	#[clap(short = 't', long = "tiling", default_value_t = 1)]
	tiling: u16,
}

pub fn edit(edit: Edit) {
	let source = match Dataset::load(&edit.input) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("Error loading data source: {:?}", err);
			return;
		},
	};

	if edit.resolution % edit.tiling != 0 {
		eprintln!("Resolution must be a multiple of tiling");
		return;
	}

	let source_metadata = source.metadata();
	let metadata = TileMetadata {
		version: FORMAT_VERSION,
		resolution: edit.resolution,
		height_resolution: edit.height_resolution,
		tiling: edit.tiling,
	};

	let needs_resize = metadata.resolution != source_metadata.resolution;

	let resizer = ThreadLocal::new();

	for_tile_in_output(
		&edit.output,
		edit.compression_level,
		&[],
		metadata,
		|lat, lon, builder| {
			if let Some(source) = source.get_tile(lat, lon).transpose()? {
				let data = if needs_resize {
					let mut resizer = resizer
						.get_or(|| {
							RefCell::new(
								Resizer::new(
									source_metadata.resolution as _,
									source_metadata.resolution as _,
									metadata.resolution as _,
									metadata.resolution as _,
									Gray16,
									Type::Lanczos3,
								)
								.unwrap(),
							)
						})
						.borrow_mut();

					let mut count = 0;
					let water_mask: Vec<_> = source
						.iter()
						.copied()
						.map(|x| {
							if x == -500 {
								count += 1;
								1
							} else {
								0
							}
						})
						.collect();

					if count != source.len() {
						let mut count = 0;
						let avg = source
							.iter()
							.copied()
							.filter_map(|x| {
								(x != -500).then(|| {
									count += 1;
									x as i64
								})
							})
							.sum::<i64>() / count + 500;
						let source: Vec<_> = source
							.into_iter()
							.map(|x| if x == -500 { avg as i16 } else { x + 500 } as u16)
							.collect();
						let res = metadata.resolution as usize * metadata.resolution as usize;
						let mut water = vec![0; res];
						let mut output = vec![0; res];

						{
							tracy::zone!("Downsample");
							resizer.resize(water_mask.as_gray(), water.as_gray_mut()).unwrap();
							resizer.resize(source.as_gray(), output.as_gray_mut()).unwrap();
						}

						if !water.iter().copied().all(|x| x == 1) {
							let output = output
								.into_iter()
								.zip(water.into_iter())
								.map(|(height, water)| if water == 1 { -500 } else { height as i16 - 500 })
								.collect();
							Some(output)
						} else {
							None
						}
					} else {
						None
					}
				} else {
					Some(source)
				};
				if let Some(data) = data {
					builder.add_tile(lat, lon, data)?;
				}
			}

			Ok(())
		},
	);
}
