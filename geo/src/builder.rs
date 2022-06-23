use std::{
	collections::{HashMap, HashSet},
	fs::{File, OpenOptions},
	io::{Seek, SeekFrom, Write},
	path::Path,
	sync::RwLock,
};

use libwebp_sys::{
	WebPEncode,
	WebPImageHint,
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
		let predicted = Self::transform_prediction(self.metadata.resolution as _, mapped);

		let compressed = Self::compress_webp(predicted, self.metadata.resolution)?;

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

	fn transform_map(height_res: u16, mut data: Vec<i16>) -> Vec<i16> {
		tracy::zone!("Map height");

		for height in &mut data {
			let h = *height + 500;
			let h = h as f32 / height_res as f32;
			*height = h.round() as i16
		}

		data
	}

	fn transform_prediction(res: usize, mut data: Vec<i16>) -> Vec<i16> {
		tracy::zone!("Map prediction");

		fn delta(previous: i16, actual: i16) -> i16 {
			if actual == -500 {
				7000
			} else {
				actual - previous
			}
		}

		fn predict_linear(previous: i16, current: i16, actual: i16) -> i16 {
			if actual == -500 {
				7000
			} else {
				let delta = current - previous;
				let pred = current + delta;
				actual - pred
			}
		}

		fn predict_plane(left: i16, above: i16, top_left: i16, actual: i16) -> i16 {
			if actual == -500 {
				7000
			} else {
				let dhdy = left - top_left;
				let pred = above + dhdy;
				actual - pred
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

	/*fn try_palette(data: Vec<i16>) -> Vec<u8> {
		tracy::zone!("Palette");

		let mut uniques = HashSet::with_capacity(256);
		let mut min = 0;
		let mut max = 0;
		for &value in data.iter() {
			uniques.insert(value);
			min = min.min(value);
			max = max.max(value)
		}

		if uniques.len() > 256 {
			if min >= i8::MIN as i16 && max <= i8::MAX as i16 {
				data.into_iter().flat_map(|x| (x as i8).to_le_bytes()).collect()
			} else {
				data.into_iter().flat_map(|x| x.to_le_bytes()).collect()
			}
		} else {
			let mut map = HashMap::with_capacity(uniques.len());
			let mut sorted: Vec<_> = uniques.into_iter().collect();
			sorted.sort_unstable();

			for (i, &x) in sorted.iter().enumerate() {
				map.insert(x, i as u8);
			}

			for offset in (0..sorted.len() - 1).rev() {
				sorted[offset + 1] -= sorted[offset];
			}

			let iter: Vec<_> = if min >= i8::MIN as i16 && max <= i8::MAX as i16 {
				sorted.into_iter().flat_map(|x| (x as i8).to_le_bytes()).collect()
			} else {
				sorted.into_iter().flat_map(|x| x.to_le_bytes()).collect()
			};
			std::iter::once(iter.len() as u8)
				.chain(iter)
				.chain(data.into_iter().map(|h| map[&h]))
				.collect()
		}
	}*/

	/*fn compress_zstd(data: Vec<i16>) -> Result<Vec<u8>, std::io::Error> {
		let paletted = Self::try_palette(data);

		let mut temp = Vec::new();
		{
			tracy::zone!("Compress");

			let mut encoder = Encoder::new(&mut temp, 21)?;
			encoder.set_pledged_src_size(Some(paletted.len() as u64))?;
			encoder.include_magicbytes(false)?;
			encoder.include_checksum(false)?;
			encoder.long_distance_matching(true)?;
			encoder.include_dictid(false)?;
			encoder.include_contentsize(false)?;
			encoder.window_log((paletted.len() as f32).log2() as u32 + 1)?;

			encoder.write_all(&paletted)?;
			encoder.finish()?;
		}

		Ok(temp)
	}*/

	fn compress_webp(data: Vec<i16>, res: u16) -> Result<Vec<u8>, std::io::Error> {
		let mut min = 0;
		let mut max = 0;
		for &value in data.iter() {
			min = min.min(value);
			max = max.max(value);
		}

		let data: Vec<_> = if max <= i8::MAX as i16 && min >= i8::MIN as i16 {
            data.into_iter().flat_map(|x| (x as i8).to_le_bytes()).collect()
        } else {
			data.into_iter().flat_map(|x| x.to_le_bytes()).collect()
		};

		unsafe {
			tracy::zone!("Compress");

			let mut temp = Vec::new();

			let mut config = std::mem::zeroed();
			WebPInitConfig(&mut config);
			config.lossless = 1;
			config.quality = 100.0;
			config.method = 3;
			config.image_hint = WebPImageHint::WEBP_HINT_DEFAULT;
			config.exact = 1;

			let mut picture = std::mem::zeroed();
			WebPPictureInit(&mut picture);
			picture.use_argb = 1;
			picture.writer = Some(write);
			picture.custom_ptr = &mut temp as *mut _ as _;
			picture.width = res as i32 / 2;
			picture.height = if data.len() == (res * res) as usize {
				res as i32 / 2
			} else {
				res as _
			};

			WebPPictureImportRGBA(&mut picture, data.as_ptr() as _, res as i32 * 2);

			WebPEncode(&config, &mut picture);

			if picture.error_code as i32 != 0 {
				return Err(std::io::Error::new(
					std::io::ErrorKind::Other,
					format!("WebPEncode failed: {}", picture.error_code as i32),
				));
			}

			unsafe extern "C" fn write(data: *const u8, data_size: usize, picture: *const WebPPicture) -> i32 {
				let vec = &mut *((*picture).custom_ptr as *mut Vec<u8>);
				vec.extend_from_slice(std::slice::from_raw_parts(data, data_size));

				1
			}

			Ok(temp)
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
