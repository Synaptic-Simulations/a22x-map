use std::{
	fs::{File, OpenOptions},
	io::{Seek, SeekFrom, Write},
	path::Path,
	sync::RwLock,
};

use zstd::{dict::EncoderDictionary, Encoder};

use crate::{map_lat_lon_to_index, Dataset, Hillshade, HillshadeMetadata, TileMetadata, FORMAT_VERSION};

struct Locked {
	tile_map: Vec<u64>,
	file: File,
}

pub struct DatasetBuilder {
	metadata: TileMetadata,
	dictionary: EncoderDictionary<'static>,
	locked: RwLock<Locked>,
}

impl DatasetBuilder {
	pub fn from_dataset(
		path: &Path, dataset: Dataset, compression_level: i8, dictionary: &[u8],
	) -> Result<Self, std::io::Error> {
		let metadata = dataset.metadata;
		let tile_map = dataset.tile_map;
		drop(dataset.data);

		Ok(Self {
			metadata,
			dictionary: EncoderDictionary::copy(dictionary, compression_level as _),
			locked: RwLock::new(Locked {
				tile_map,
				file: OpenOptions::new().write(true).read(true).open(path)?,
			}),
		})
	}

	pub fn new(
		path: &Path, metadata: TileMetadata, compression_level: i8, dictionary: &[u8],
	) -> Result<Self, std::io::Error> {
		assert_eq!(
			metadata.version, FORMAT_VERSION,
			"Can only build datasets with version {}",
			FORMAT_VERSION
		);
		assert_eq!(
			metadata.resolution % metadata.tiling,
			0,
			"Resolution must be a multiple of tiling"
		);

		let tile_map = vec![0; 360 * 180];

		let mut file = File::create(path)?;
		Self::write_to_file(&mut file, metadata, &tile_map, &[], dictionary)?;

		Ok(Self {
			metadata,
			dictionary: EncoderDictionary::copy(dictionary, compression_level as _),
			locked: RwLock::new(Locked { tile_map, file }),
		})
	}

	pub fn tile_exists(&self, lat: i16, lon: i16) -> bool {
		let index = map_lat_lon_to_index(lat, lon);
		self.locked.read().unwrap().tile_map[index] != 0
	}

	pub fn add_tile_from_dataset(&self, lat: i16, lon: i16, dataset: &Dataset) -> Result<(), std::io::Error> {
		assert_eq!(
			self.metadata.version, FORMAT_VERSION,
			"Can only add tiles to datasets with version {}",
			FORMAT_VERSION
		);
		assert_eq!(
			dataset.metadata.resolution, self.metadata.resolution,
			"Resolutions must match"
		);
		assert_eq!(
			dataset.metadata.height_resolution, self.metadata.height_resolution,
			"Height resolutions must match"
		);
		assert_eq!(dataset.metadata.tiling, self.metadata.tiling, "Tilings must match");
		assert_eq!(
			dataset.dictionary.as_ddict().get_dict_id(),
			self.dictionary.as_cdict().get_dict_id(),
			"Dictionaries must match"
		);

		let index = map_lat_lon_to_index(lat, lon);
		let data_offset = dataset.tile_map[index] as usize;
		let data_size = dataset.get_tile_compressed_size(lat, lon);
		let offset = data_offset - dataset.data_offset;
		let data = &dataset.data[offset..offset + data_size];

		let mut locked = self.locked.write().unwrap();
		let offset = locked.file.seek(SeekFrom::End(0))?;
		locked.file.write_all(&data)?;
		locked.tile_map[index] = offset;

		Ok(())
	}

	pub fn add_tile(&self, lat: i16, lon: i16, data: Vec<i16>) -> Result<(), std::io::Error> {
		let data: Vec<_> = {
			tracy::zone!("Tile");

			let tiling = self.metadata.tiling as usize;
			let tile_size = tiling * tiling;
			let tiles_per_row = self.metadata.resolution as usize / tiling;
			let mut out = vec![0; data.len() * 2];

			for (i, tile) in out.chunks_exact_mut(tile_size * 2).enumerate() {
				let tile_offset_x = (i % tiles_per_row) * tiling;
				let tile_offset_y = (i / tiles_per_row) * tiling;
				for (j, value) in tile.chunks_exact_mut(2).enumerate() {
					let x = tile_offset_x + j % tiling;
					let y = tile_offset_y + j / tiling;
					let height = data[y * self.metadata.resolution as usize + x];

					let positive_height = height + 500;
					let height = positive_height as f32 / self.metadata.height_resolution as f32;
					value.copy_from_slice(&(height.round() as u16).to_le_bytes());
				}
			}

			out
		};

		let mut temp = Vec::new();
		{
			tracy::zone!("Compress");

			let mut encoder = Encoder::with_prepared_dictionary(&mut temp, &self.dictionary)?;
			encoder.set_pledged_src_size(Some(data.len() as u64))?;
			encoder.include_magicbytes(false)?;
			encoder.include_checksum(false)?;
			encoder.long_distance_matching(true)?;
			encoder.include_dictid(false)?;
			encoder.include_contentsize(false)?;

			encoder.write_all(&data)?;
			encoder.finish()?;
		}

		tracy::zone!("Write");

		let index = map_lat_lon_to_index(lat, lon);
		let mut locked = self.locked.write().unwrap();
		let offset = locked.file.seek(SeekFrom::End(0))?;
		locked.file.write_all(&temp)?;
		locked.tile_map[index] = offset;

		Ok(())
	}

	pub fn flush(&self) -> Result<(), std::io::Error> {
		tracy::zone!("Flush");

		let mut locked = self.locked.write().unwrap();

		locked.file.seek(SeekFrom::Start(match self.metadata.version {
			3 => Dataset::VER3_TILE_MAP_OFFSET,
			4 => Dataset::VER4_TILE_MAP_OFFSET,
			_ => unreachable!(),
		} as _))?;
		let slice = unsafe { std::slice::from_raw_parts(locked.tile_map.as_ptr() as _, locked.tile_map.len() * 8) };
		locked.file.write_all(slice)?;

		locked.file.flush()?;

		Ok(())
	}

	pub fn finish(self) -> Result<(), std::io::Error> { self.flush() }

	fn write_to_file(
		file: &mut File, metadata: TileMetadata, tile_map: &[u64], data: &[u8], dictionary: &[u8],
	) -> Result<(), std::io::Error> {
		let mut header = [0; Dataset::VER4_TILE_MAP_OFFSET];
		header[0..5].copy_from_slice(&Dataset::MAGIC);
		header[5..7].copy_from_slice(&metadata.version.to_le_bytes());
		header[7..9].copy_from_slice(&metadata.resolution.to_le_bytes());
		header[9..11].copy_from_slice(&metadata.height_resolution.to_le_bytes());
		header[11..13].copy_from_slice(&metadata.tiling.to_le_bytes());

		file.write_all(&header)?;
		file.write_all(&dictionary.len().to_le_bytes())?;
		file.write_all(&dictionary)?;
		file.write_all(unsafe { std::slice::from_raw_parts(tile_map.as_ptr() as _, tile_map.len() * 8) })?;
		file.write_all(&data)?;

		Ok(())
	}
}

pub struct HillshadeBuilder {
	compression_level: i8,
	metadata: HillshadeMetadata,
	locked: RwLock<Locked>,
}

impl HillshadeBuilder {
	pub fn from_dataset(path: &Path, dataset: Hillshade, compression_level: i8) -> Result<Self, std::io::Error> {
		let metadata = dataset.metadata;
		let tile_map = dataset.tile_map;
		drop(dataset.data);

		Ok(Self {
			compression_level,
			metadata,
			locked: RwLock::new(Locked {
				tile_map,
				file: OpenOptions::new().write(true).read(true).open(path)?,
			}),
		})
	}

	pub fn new(path: &Path, metadata: HillshadeMetadata, compression_level: i8) -> Result<Self, std::io::Error> {
		assert_eq!(
			metadata.version, FORMAT_VERSION,
			"Can only build datasets with version {}",
			FORMAT_VERSION
		);
		assert_eq!(
			metadata.resolution % metadata.tiling,
			0,
			"Resolution must be a multiple of tiling"
		);

		let tile_map = vec![0; 360 * 180];

		let mut file = File::create(path)?;
		Self::write_to_file(&mut file, metadata, &tile_map, &[])?;

		Ok(Self {
			compression_level,
			metadata,
			locked: RwLock::new(Locked { tile_map, file }),
		})
	}

	pub fn tile_exists(&self, lat: i16, lon: i16) -> bool {
		let index = map_lat_lon_to_index(lat, lon);
		self.locked.read().unwrap().tile_map[index] != 0
	}

	pub fn add_tile(&self, lat: i16, lon: i16, data: Vec<u8>) -> Result<(), std::io::Error> {
		let data: Vec<_> = {
			tracy::zone!("Tile");

			let tiling = self.metadata.tiling as usize;
			let tile_size = tiling * tiling;
			let tiles_per_row = self.metadata.resolution as usize / tiling;
			let mut out = vec![0; data.len()];

			for (i, tile) in out.chunks_exact_mut(tile_size).enumerate() {
				let tile_offset_x = (i % tiles_per_row) * tiling;
				let tile_offset_y = (i / tiles_per_row) * tiling;
				for (j, value) in tile.iter_mut().enumerate() {
					let x = tile_offset_x + j % tiling;
					let y = tile_offset_y + j / tiling;

					*value = data[y * self.metadata.resolution as usize + x]
				}
			}

			out
		};

		let mut temp = Vec::new();
		{
			tracy::zone!("Compress");

			let mut encoder = Encoder::new(&mut temp, self.compression_level as _).expect("Compression error");
			encoder.set_pledged_src_size(Some(data.len() as u64)).unwrap();
			encoder.include_magicbytes(false).unwrap();
			encoder.include_checksum(false).unwrap();
			encoder.long_distance_matching(true).unwrap();

			encoder.write_all(&data).unwrap();
			encoder.finish().unwrap();
		}

		tracy::zone!("Write");

		let index = map_lat_lon_to_index(lat, lon);
		let mut locked = self.locked.write().unwrap();
		let offset = locked.file.seek(SeekFrom::End(0))?;
		locked.file.write_all(&temp)?;
		locked.tile_map[index] = offset;

		Ok(())
	}

	pub fn flush(&self) -> Result<(), std::io::Error> {
		tracy::zone!("Flush");

		let mut locked = self.locked.write().unwrap();

		locked.file.seek(SeekFrom::Start(Hillshade::TILE_MAP_OFFSET as _))?;
		let slice = unsafe { std::slice::from_raw_parts(locked.tile_map.as_ptr() as _, locked.tile_map.len() * 8) };
		locked.file.write_all(slice)?;

		locked.file.flush()?;

		Ok(())
	}

	pub fn finish(self) -> Result<(), std::io::Error> { self.flush() }

	fn write_to_file(
		file: &mut File, metadata: HillshadeMetadata, tile_map: &[u64], data: &[u8],
	) -> Result<(), std::io::Error> {
		let mut header = [0; Hillshade::TILE_MAP_OFFSET];
		header[0..5].copy_from_slice(&Dataset::MAGIC);
		header[5..7].copy_from_slice(&metadata.version.to_le_bytes());
		header[7..9].copy_from_slice(&metadata.resolution.to_le_bytes());
		header[9..11].copy_from_slice(&metadata.tiling.to_le_bytes());

		file.write_all(&header)?;
		file.write_all(&0u64.to_le_bytes())?;
		file.write_all(unsafe { std::slice::from_raw_parts(tile_map.as_ptr() as _, tile_map.len() * 8) })?;
		file.write_all(&data)?;

		Ok(())
	}
}
