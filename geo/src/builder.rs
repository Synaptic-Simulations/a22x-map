use std::{
	collections::{HashMap, HashSet},
	fs::{File, OpenOptions},
	io::{Seek, SeekFrom, Write},
	path::Path,
	sync::RwLock,
};

use zstd::Encoder;

use crate::{map_lat_lon_to_index, Dataset, TileMetadata, FORMAT_VERSION};

struct Locked {
	tile_map: Vec<u64>,
	file: File,
}

pub struct DatasetBuilder {
	metadata: TileMetadata,
	locked: RwLock<Locked>,
}

impl DatasetBuilder {
	pub fn from_dataset(path: &Path, dataset: Dataset) -> Result<Self, std::io::Error> {
		let metadata = dataset.metadata;
		let tile_map = dataset.tile_map;
		drop(dataset.data);

		Ok(Self {
			metadata,
			locked: RwLock::new(Locked {
				tile_map,
				file: OpenOptions::new().write(true).read(true).open(path)?,
			}),
		})
	}

	pub fn new(path: &Path, metadata: TileMetadata) -> Result<Self, std::io::Error> {
		assert_eq!(
			metadata.version, FORMAT_VERSION,
			"Can only build datasets with version {}",
			FORMAT_VERSION
		);

		let tile_map = vec![0; 360 * 180];

		let mut file = File::create(path)?;
		Self::write_to_file(&mut file, metadata, &tile_map, &[])?;

		Ok(Self {
			metadata,
			locked: RwLock::new(Locked { tile_map, file }),
		})
	}

	pub fn tile_exists(&self, lat: i16, lon: i16) -> bool {
		let index = map_lat_lon_to_index(lat, lon);
		self.locked.read().unwrap().tile_map[index] != 0
	}

	pub fn add_tile(&self, lat: i16, lon: i16, data: Vec<i16>) -> Result<(), std::io::Error> {
		let mapped = Self::transform_map(self.metadata.height_resolution, data);
		let predicted = Self::transform_prediction(self.metadata.resolution as _, mapped);
		let paletted = Self::palette(predicted);
		let compressed = Self::compress(paletted)?;

		tracy::zone!("Write");
		let index = map_lat_lon_to_index(lat, lon);
		let mut locked = self.locked.write().unwrap();
		let offset = locked.file.seek(SeekFrom::End(0))?;
		locked.tile_map[index] = offset;
		locked.file.write_all(&compressed)
	}

	pub fn flush(&self) -> Result<(), std::io::Error> {
		tracy::zone!("Flush");

		let mut locked = self.locked.write().unwrap();

		locked.file.seek(SeekFrom::Start(match self.metadata.version {
			3 => Dataset::VER3_TILE_MAP_OFFSET,
			4 => Dataset::VER4_TILE_MAP_OFFSET,
			5 => Dataset::VER5_TILE_MAP_OFFSET,
			_ => unreachable!(),
		} as _))?;
		let slice = unsafe { std::slice::from_raw_parts(locked.tile_map.as_ptr() as _, locked.tile_map.len() * 8) };
		locked.file.write_all(slice)?;

		locked.file.flush()?;

		Ok(())
	}

	pub fn finish(self) -> Result<(), std::io::Error> { self.flush() }

	fn transform_map(height_res: u16, data: Vec<i16>) -> Vec<u16> {
		tracy::zone!("Heightmap");

		data.into_iter()
			.map(|height| {
				let h = height + 500;
				let h = h as f32 / height_res as f32;
				h.round() as u16
			})
			.collect()
	}

	fn transform_prediction(res: usize, mut data: Vec<u16>) -> Vec<u16> {
		tracy::zone!("Map prediction");

		fn delta(previous: u16, actual: u16) -> u16 {
			if actual == 0 {
				// water
				0
			} else {
				let signed = actual as i16 - previous as i16;
				(signed + 7000) as u16
			}
		}

		fn predict_linear(previous: u16, current: u16, actual: u16) -> u16 {
			if actual == 0 {
				0
			} else {
				let delta = current as i16 - previous as i16;
				let pred = current as i16 + delta;
				let signed = actual as i16 - pred;
				(signed + 7000) as u16
			}
		}

		fn predict_plane(left: u16, above: u16, top_left: u16, actual: u16) -> u16 {
			if actual == 0 {
				0
			} else {
				let dhdy = left as i16 - top_left as i16;
				let pred = above as i16 + dhdy;
				let signed = actual as i16 - pred;
				(signed + 7000) as u16
			}
		}

		// Predict everything except the first row and column, bottom to top, right to left.
		for x in (1..res).rev() {
			for y in (1..res).rev() {
				let left = data[y * res + x - 1];
				let above = data[(y - 1) * res + x];
				let top_left = data[(y - 1) * res + x - 1];
				let actual = data[y * res + x];
				data[y * res + x] = predict_plane(left, above, top_left, actual);
			}
		}
		// Predict the first row and column, except for (1, 0) and (0, 1).
		for x in (2..res).rev() {
			let previous = data[x - 2];
			let current = data[x - 1];
			let actual = data[x];
			data[x] = predict_linear(previous, current, actual);
		}
		for y in (2..res).rev() {
			let previous = data[(y - 2) * res];
			let current = data[(y - 1) * res];
			let actual = data[y * res];
			data[y * res] = predict_linear(previous, current, actual);
		}

		// Predict (0, 1) and (1, 0).
		data[1] = delta(data[0], data[1]);
		data[res] = delta(data[0], data[res]);

		data
	}

	fn palette(data: Vec<u16>) -> Vec<u8> {
		tracy::zone!("Palette");

		let mut uniques = HashSet::with_capacity(256);
		let mut min = 0;
		let mut max = 0;
		for &value in data[1..].iter() {
			uniques.insert(value);
			min = min.min(value);
			max = max.max(value);
		}

		if uniques.len() > 256 {
			if max - min < u8::MAX as u16 {
				std::iter::once(min.to_le_bytes())
					.flatten()
					.chain(std::iter::once(data[0].to_le_bytes()).flatten())
					.chain(data[1..].iter().map(|x| if *x == 0 { 0 } else { (*x - min + 1) as u8 }))
					.collect()
			} else {
				std::iter::once(min.to_le_bytes())
					.flatten()
					.chain(data.into_iter().flat_map(|x| (x - min).to_le_bytes()))
					.collect()
			}
		} else {
			let mut map = HashMap::with_capacity(uniques.len());
			let mut sorted: Vec<_> = uniques.into_iter().collect();
			sorted.sort_unstable();

			for (i, &x) in sorted.iter().enumerate() {
				map.insert(x, i as u8);
			}

			let mut max = 0;
			for offset in (0..sorted.len() - 1).rev() {
				sorted[offset + 1] -= sorted[offset];
				max = max.max(sorted[offset + 1]);
			}

			let len = sorted.len();
			let palette: Vec<_> = if max <= u8::MAX as u16 {
				sorted.into_iter().map(|x| x as u8).collect()
			} else {
				sorted.into_iter().flat_map(|x| x.to_le_bytes()).collect()
			};

			std::iter::once(len as u8)
				.chain(palette)
				.chain(std::iter::once(data[0].to_le_bytes()).flatten())
				.chain(data[1..].iter().map(|h| map[h]))
				.collect()
		}
	}

	fn compress(data: Vec<u8>) -> Result<Vec<u8>, std::io::Error> {
		let mut temp = Vec::new();
		{
			tracy::zone!("Compress");

			let mut encoder = Encoder::new(&mut temp, 22)?;
			encoder.set_pledged_src_size(Some(data.len() as u64))?;
			encoder.include_magicbytes(false)?;
			encoder.include_checksum(false)?;
			encoder.long_distance_matching(true)?;
			encoder.include_dictid(false)?;
			encoder.include_contentsize(false)?;
			encoder.window_log(24)?;

			encoder.write_all(&data)?;
			encoder.finish()?;
		}

		Ok(temp)
	}

	fn write_to_file(
		file: &mut File, metadata: TileMetadata, tile_map: &[u64], data: &[u8],
	) -> Result<(), std::io::Error> {
		let mut header = [0; Dataset::VER5_TILE_MAP_OFFSET];
		header[0..5].copy_from_slice(&Dataset::MAGIC);
		header[5..7].copy_from_slice(&metadata.version.to_le_bytes());
		header[7..9].copy_from_slice(&metadata.resolution.to_le_bytes());
		header[9..11].copy_from_slice(&metadata.height_resolution.to_le_bytes());
		header[11] = metadata.delta_compressed as u8;

		file.write_all(&header)?;
		file.write_all(&0u64.to_le_bytes())?;
		file.write_all(unsafe { std::slice::from_raw_parts(tile_map.as_ptr() as _, tile_map.len() * 8) })?;
		file.write_all(&data)?;

		Ok(())
	}
}
