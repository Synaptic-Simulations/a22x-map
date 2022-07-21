use std::{cell::RefCell, path::PathBuf};

use clap::Args;
use geo::{Dataset, TileMetadata, FORMAT_VERSION};
use resize::{
	Pixel::{Gray16, Gray8},
	Resizer,
	Type,
};
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
}

pub fn edit(edit: Edit) {
	let source = match Dataset::load(&edit.input) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("Error loading data source: {:?}", err);
			return;
		},
	};

	let source_metadata = source.metadata();
	let metadata = TileMetadata {
		version: FORMAT_VERSION,
		resolution: edit.resolution,
		height_resolution: edit.height_resolution,
	};

	let needs_resize = metadata.resolution != source_metadata.resolution;

	let u16_resize = ThreadLocal::new();
	let u8_resize = ThreadLocal::new();

	for_tile_in_output(&edit.output, metadata, |lat, lon, builder| {
		if let Some((data, water, hillshade)) = source.get_full_tile(lat, lon).transpose()? {
			let data = if needs_resize {
				let mut u16_resize = u16_resize
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
				let mut u8_resize = u8_resize
					.get_or(|| {
						RefCell::new(
							Resizer::new(
								source_metadata.resolution as _,
								source_metadata.resolution as _,
								metadata.resolution as _,
								metadata.resolution as _,
								Gray8,
								Type::Lanczos3,
							)
							.unwrap(),
						)
					})
					.borrow_mut();

				let res = metadata.resolution as usize;
				let mut data_out = vec![0; res * res];
				let mut water_out = vec![0; res * res];
				let mut hillshade_out = vec![0; res * res];

				let _ = u16_resize.resize(data.as_gray(), data_out.as_gray_mut());
				let _ = u8_resize.resize(water.as_gray(), water_out.as_gray_mut());
				let _ = u8_resize.resize(hillshade.as_gray(), hillshade_out.as_gray_mut());

				if water_out.iter().all(|&x| x == 1) {
					None
				} else {
					Some((data_out, water_out, hillshade_out))
				}
			} else {
				Some((data, water, hillshade))
			};

			if let Some(data) = data {
				builder.add_tile(lat, lon, data.0, data.1, data.2)?;
			}
		}

		Ok(())
	});
}
