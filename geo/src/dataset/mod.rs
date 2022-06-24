use std::{fs::File, io::Read, path::Path};

use memmap2::Mmap;

use crate::{map_lat_lon_to_index, LoadError, TileMetadata};

mod ver3;
mod ver4;
mod ver5;
mod ver6;

pub struct Dataset {
	pub(crate) metadata: TileMetadata,
	pub(crate) tile_map: Vec<u64>,
	pub(crate) data: Mmap,
	pub(crate) data_offset: usize,
}

impl Dataset {
	pub(crate) const MAGIC: [u8; 5] = [115, 117, 115, 115, 121];
	pub(crate) const VER3_DICT_OFFSET: usize = Self::VER3_TILE_MAP_OFFSET + 360 * 180 * 8;
	pub(crate) const VER3_TILE_MAP_OFFSET: usize = 11;
	pub(crate) const VER4_DICT_OFFSET: usize = Self::VER4_TILE_MAP_OFFSET + 360 * 180 * 8;
	pub(crate) const VER4_TILE_MAP_OFFSET: usize = 13;
	pub(crate) const VER5_TILE_MAP_OFFSET: usize = 12;

	pub fn load(dir: &Path) -> Result<Self, LoadError> {
		let meta = std::fs::metadata(&dir)?;
		if meta.is_dir() {
			Err(LoadError::UnsupportedFormatVersion)
		} else {
			let mut file = File::open(dir)?;
			let mut buffer = Vec::with_capacity(Self::VER4_DICT_OFFSET + 8); // max dict offset + 8
			buffer.resize(buffer.capacity(), 0);

			file.read_exact(&mut buffer[0..7])
				.map_err(|_| LoadError::InvalidFileSize)?;
			if buffer[0..5] != Self::MAGIC {
				return Err(LoadError::InvalidMagic);
			}
			let version = u16::from_le_bytes(buffer[5..7].try_into().unwrap());
			match version {
				3 => ver3::load(&mut buffer, &mut file),
				4 => ver4::load(&mut buffer, &mut file),
				5 => ver5::load(&mut buffer, &mut file),
				6 => ver6::load(&mut buffer, &mut file),
				_ => Err(LoadError::UnsupportedFormatVersion),
			}
		}
	}

	pub fn metadata(&self) -> TileMetadata { self.metadata }

	pub fn tile_exists(&self, lat: i16, lon: i16) -> bool {
		let index = map_lat_lon_to_index(lat, lon);
		self.tile_map[index] != 0
	}

	pub fn tile_count(&self) -> usize { self.tile_map.iter().filter(|&&x| x != 0).count() }

	pub fn get_tile(&self, lat: i16, lon: i16) -> Option<Result<Vec<i16>, std::io::Error>> {
		self.get_tile_and_compressed_size(lat, lon).map(|x| x.map(|(x, _)| x))
	}

	pub fn get_tile_and_compressed_size(
		&self, lat: i16, lon: i16,
	) -> Option<Result<(Vec<i16>, usize), std::io::Error>> {
		tracy::zone!("Get Tile");
		match self.metadata.version {
			3 => ver4::get_tile(self, lat, lon),
			4 => ver4::get_tile(self, lat, lon),
			5 => ver5::get_tile(self, lat, lon),
			6 => ver6::get_tile(self, lat, lon),
			_ => unreachable!(),
		}
	}
}
