use std::{
	collections::{HashMap, HashSet},
	fs::{File, OpenOptions},
	io::{Seek, SeekFrom, Write},
	path::Path,
	sync::RwLock,
};

use libwebp_sys::{
	WebPEncode,
	WebPImageHint::WEBP_HINT_GRAPH,
	WebPInitConfig,
	WebPPicture,
	WebPPictureImportRGBA,
	WebPPictureInit,
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
		let predicted: Vec<_> = Self::transform_prediction(self.metadata.resolution as _, mapped);
		let paletted = Self::try_palette(predicted);

		let mut temp = Vec::new();
		{
			tracy::zone!("Compress");

			let mut encoder = Encoder::new(&mut temp, 0)?;
			encoder.set_pledged_src_size(Some(paletted.len() as u64))?;
			encoder.include_magicbytes(false)?;
			encoder.include_checksum(false)?;
			encoder.long_distance_matching(true)?;
			encoder.include_dictid(false)?;
			encoder.include_contentsize(false)?;

			encoder.write_all(&paletted)?;
			encoder.finish()?;
		}

		tracy::zone!("Write");

		let index = map_lat_lon_to_index(lat, lon);
		let mut locked = self.locked.write().unwrap();
		let offset = locked.file.seek(SeekFrom::End(0))?;
		locked.tile_map[index] = offset;
		locked.file.write_all(&temp)
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

	fn transform_map(height_res: u16, mut data: Vec<i16>) -> Vec<i16> {
		for height in &mut data {
			let h = *height + 500;
			let h = h as f32 / height_res as f32;
			*height = h.round() as i16
		}

		data
	}

	fn transform_prediction(res: usize, mut data: Vec<i16>) -> Vec<i16> {
		fn predict(previous: i16, current: i16) -> i16 {
			let delta = current - previous;
			current + delta
		}

		// Starting from top left, store delta to the bottom.
		data[res] -= data[0];
		// Then predict first column.
		for row in 2..res {
			data[row * res] -= predict(data[(row - 2) * res], data[(row - 1) * res]);
		}
		// Then predict each row.
		for row in data.chunks_exact_mut(res) {
			// Store second as delta from the left.
			row[1] -= row[0];
			for offset in 0..res - 2 {
				row[offset + 2] -= predict(row[offset], row[offset + 1]);
			}
		}

		data
	}

	fn try_palette(data: Vec<i16>) -> Vec<u8> {
		let mut uniques = HashSet::with_capacity(256);
		for value in data.iter() {
			uniques.insert(*value);
		}

		if uniques.len() > 256 {
			data.into_iter().flat_map(|x| x.to_le_bytes()).collect()
		} else {
			let mut map = HashMap::with_capacity(uniques.len());
			let mut sorted: Vec<_> = uniques.into_iter().collect();
			sorted.sort_unstable();

			for (i, &x) in sorted.iter().enumerate() {
				map.insert(x, i as u8);
			}

			for offset in 0..sorted.len() {
				sorted[offset + 1] -= sorted[offset];
			}

			std::iter::once(sorted.len() as u8)
				.chain(sorted.into_iter().flat_map(|x| x.to_le_bytes()))
				.chain(data.into_iter().map(|h| map[&h]))
				.collect()
		}
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
