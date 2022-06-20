use std::{
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
		let mapped: Vec<_> = data
			.into_iter()
			.map(|height| {
				let h = height + 500;
				let h = h as f32 / self.metadata.height_resolution as f32;
				h.round() as i16
			})
			.collect();

		let data: Vec<u8> = if self.metadata.delta_compressed {
			std::iter::once(mapped[0].to_le_bytes())
				.flatten()
				.chain(mapped.windows(2).flat_map(|x| {
					let l = x[0];
					let r = x[1];

					(r - l).to_le_bytes()
				}))
				.collect()
		} else {
			mapped.into_iter().flat_map(i16::to_le_bytes).collect()
		};

		let mut temp = Vec::new();
		unsafe {
			tracy::zone!("Compress");

			let mut config = std::mem::zeroed();
			WebPInitConfig(&mut config);
			config.lossless = 1;
			config.quality = 100.0;
			config.method = 3;
			config.image_hint = WEBP_HINT_GRAPH;
			config.exact = 1;

			let mut picture = std::mem::zeroed();
			WebPPictureInit(&mut picture);
			picture.use_argb = 1;
			picture.writer = Some(write);
			picture.custom_ptr = &mut temp as *mut _ as _;
			picture.width = self.metadata.resolution as i32 / 2;
			picture.height = self.metadata.resolution as _;

			WebPPictureImportRGBA(&mut picture, data.as_ptr() as _, self.metadata.resolution as i32 * 2);

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
