use std::path::PathBuf;

use clap::Args;
use geo::{TileMetadata, FORMAT_VERSION};

use crate::{
	common::for_tile_in_output,
	source::{LatLon, Raster},
};

#[derive(Args)]
/// Generate a dataset from a raw source.
pub struct Generate {
	input: PathBuf,
	#[clap(short = 'w', long = "water")]
	water: PathBuf,
	#[clap(short = 'o', long = "out")]
	output: PathBuf,
	#[clap(short = 'r', long = "res", default_value_t = 1200)]
	resolution: u16,
	#[clap(short = 's', long = "hres", default_value_t = 1)]
	height_resolution: u16,
}

pub fn generate(generate: Generate) {
	let source = match Raster::load(&generate.input) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("Error loading data source: {:?}", err);
			return;
		},
	};
	let water = match Raster::load(&generate.water) {
		Ok(source) => source,
		Err(err) => {
			eprintln!("Error loading water source: {:?}", err);
			return;
		},
	};
	let metadata = TileMetadata {
		version: FORMAT_VERSION,
		resolution: generate.resolution,
		height_resolution: generate.height_resolution,
	};

	for_tile_in_output(&generate.output, metadata, |lat, lon, builder| {
		let bottom_left = LatLon {
			lat: lat as f64,
			lon: lon as f64,
		};
		let top_right = LatLon {
			lat: (lat + 1) as f64,
			lon: (lon + 1) as f64,
		};

		source
			.get_data_for_hillshade(bottom_left, top_right, metadata.resolution as _)
			.and_then(|(data, has_extra): (Vec<i16>, _)| {
				tracy::zone!("Load water");
				water
					.get_data(bottom_left, top_right, metadata.resolution as _)
					.map(|water: Vec<u8>| (data, has_extra, water))
			})
			.and_then(|(data, has_extra, water)| {
				let res = metadata.resolution as usize;
				assert!(res * res <= data.len());

				let (data, hillshade) = if has_extra {
					let ores = res;
					let res = res + 2;

					let hillshade = {
						tracy::zone!("Generate hillshade");

						let zenith = 45.0f32.to_radians();
						let azimuth = 135.0f32.to_radians();

						let mut out = vec![0; ores * ores];
						for x in 1..res - 1 {
							for y in 1..res - 1 {
								let a = data[(y - 1) * res + x - 1] as f32;
								let b = data[(y - 1) * res + x] as f32;
								let c = data[(y - 1) * res + x + 1] as f32;
								let d = data[y * res + x - 1] as f32;
								let f = data[y * res + x + 1] as f32;
								let g = data[(y + 1) * res + x - 1] as f32;
								let h = data[(y + 1) * res + x] as f32;
								let i = data[(y + 1) * res + x + 1] as f32;

								let dzdx = ((c + 2.0 * f + i) - (a + 2.0 * d + g)) / 8.0;
								let dzdy = ((g + 2.0 * h + i) - (a + 2.0 * b + c)) / 8.0;

								let slope = (dzdx * dzdx + dzdy * dzdy).sqrt().atan();
								let aspect = if dzdx != 0.0 {
									let aspect = dzdy.atan2(-dzdx);
									if aspect < 0.0 {
										aspect + 2.0 * std::f32::consts::PI
									} else {
										aspect
									}
								} else {
									if dzdy > 0.0 {
										0.5 * std::f32::consts::PI
									} else {
										1.5 * std::f32::consts::PI
									}
								};

								let hillshade = (zenith.cos() * slope.cos()
									+ zenith.sin() * slope.sin() * (azimuth - aspect).cos())
								.clamp(0.0, 1.0);

								out[(y - 1) * ores + x - 1] = (hillshade * 255.0).round() as u8;
							}
						}

						out
					};

					let mut out = vec![0; ores * ores];
					for x in 1..res - 1 {
						for y in 1..res - 1 {
							out[(y - 1) * ores + x - 1] = data[y * res + x];
						}
					}

					(out, hillshade)
				} else {
					let hillshade = {
						tracy::zone!("Generate hillshade");

						let zenith = 45.0f32.to_radians();
						let azimuth = 135.0f32.to_radians();

						let mut out = vec![0; res * res];
						for x in 1..res - 1 {
							for y in 1..res - 1 {
								let a = data[(y - 1) * res + x - 1] as f32;
								let b = data[(y - 1) * res + x] as f32;
								let c = data[(y - 1) * res + x + 1] as f32;
								let d = data[y * res + x - 1] as f32;
								let f = data[y * res + x + 1] as f32;
								let g = data[(y + 1) * res + x - 1] as f32;
								let h = data[(y + 1) * res + x] as f32;
								let i = data[(y + 1) * res + x + 1] as f32;

								let dzdx = ((c + 2.0 * f + i) - (a + 2.0 * d + g)) / 8.0;
								let dzdy = ((g + 2.0 * h + i) - (a + 2.0 * b + c)) / 8.0;

								let slope = (dzdx * dzdx + dzdy * dzdy).sqrt().atan();
								let aspect = if dzdx != 0.0 {
									let aspect = dzdy.atan2(-dzdx);
									if aspect < 0.0 {
										aspect + 2.0 * std::f32::consts::PI
									} else {
										aspect
									}
								} else {
									if dzdy > 0.0 {
										0.5 * std::f32::consts::PI
									} else {
										1.5 * std::f32::consts::PI
									}
								};

								let hillshade = (zenith.cos() * slope.cos()
									+ zenith.sin() * slope.sin() * (azimuth - aspect).cos())
								.clamp(0.0, 1.0);

								out[y * res + x] = (hillshade * 255.0).round() as u8;
							}
						}

						out
					};

					(data, hillshade)
				};

				let mut water_count = 0;
				let data = data
					.into_iter()
					.zip(water.iter())
					.map(|(h, &w)| {
						let positive = (h + 500) as u16;
						water_count += w as u32;
						positive
					})
					.collect();

				if water_count != metadata.resolution as u32 * metadata.resolution as u32 {
					Some(builder.add_tile(lat, lon, data, water, hillshade))
				} else {
					None
				}
			})
			.transpose()?;

		Ok(())
	});
}
