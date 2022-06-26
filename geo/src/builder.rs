use std::{
	fs::{File, OpenOptions},
	io::{Seek, SeekFrom, Write},
	path::Path,
	sync::RwLock,
};

use hcomp::{encode::encode, Heightmap};
use jpegxl_rs::encode::{EncoderFrame, EncoderSpeed, JxlEncoder};

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
		Self::write_to_file(&mut file, metadata, &tile_map)?;

		Ok(Self {
			metadata,
			locked: RwLock::new(Locked { tile_map, file }),
		})
	}

	pub fn tile_exists(&self, lat: i16, lon: i16) -> bool {
		let index = map_lat_lon_to_index(lat, lon);
		self.locked.read().unwrap().tile_map[index] != 0
	}

	/// data: heights in meters, with the 14th bit set if pixel is water.
	pub fn add_tile(&self, lat: i16, lon: i16, data: Vec<i16>) -> Result<(), std::io::Error> {
		let water: Vec<_> = {
			tracy::zone!("Water mask");
			data.iter()
				.map(|&x| {
					let x = x as u16 >> 13;
					x as u8 & 1
				})
				.collect()
		};

		let water = {
			tracy::zone!("Compress water");
			JxlEncoder {
				lossless: true,
				speed: EncoderSpeed::Tortoise,
				use_container: false,
				..Default::default()
			}
			.encode_frame(
				&EncoderFrame::new(&water).num_channels(1),
				self.metadata.resolution as _,
				self.metadata.resolution as _,
			)?
			.data
		};

		let data: Vec<_> = {
			tracy::zone!("Map height");
			data.into_iter()
				.map(|x| {
					let mask = !(1 << 13);
					let positive = (x & mask + 500) as f32;
					let mapped = positive / self.metadata.height_resolution as f32;
					mapped.round() as u16
				})
				.collect()
		};

		let data = {
			tracy::zone!("Compress height");
			let mut out = Vec::new();

			encode(
				Heightmap {
					width: self.metadata.resolution as _,
					height: self.metadata.resolution as _,
					data: data.into(),
				},
				22,
				&mut out,
			)?;

			out
		};

		tracy::zone!("Write");
		let index = map_lat_lon_to_index(lat, lon);
		let mut locked = self.locked.write().unwrap();
		let offset = locked.file.seek(SeekFrom::End(0))?;
		locked.tile_map[index] = offset;
		locked.file.write_all(&data)?;
		locked.file.write_all(&water)
	}

	pub fn flush(&self) -> Result<(), std::io::Error> {
		tracy::zone!("Flush");

		let mut locked = self.locked.write().unwrap();

		locked.file.seek(SeekFrom::Start(32))?;
		let slice = unsafe { std::slice::from_raw_parts(locked.tile_map.as_ptr() as _, locked.tile_map.len() * 8) };
		locked.file.write_all(slice)?;

		locked.file.flush()?;

		Ok(())
	}

	pub fn finish(self) -> Result<(), std::io::Error> { self.flush() }

	fn write_to_file(file: &mut File, metadata: TileMetadata, tile_map: &[u64]) -> Result<(), std::io::Error> {
		let mut header = [0; 32];
		header[0..5].copy_from_slice(&Dataset::MAGIC);
		header[5..7].copy_from_slice(&metadata.version.to_le_bytes());
		header[7..9].copy_from_slice(&metadata.resolution.to_le_bytes());
		header[9..11].copy_from_slice(&metadata.height_resolution.to_le_bytes());

		file.write_all(&header)?;
		file.write_all(unsafe { std::slice::from_raw_parts(tile_map.as_ptr() as _, tile_map.len() * 8) })?;

		Ok(())
	}
}
