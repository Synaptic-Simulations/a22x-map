use std::{collections::HashSet, fs::File, io::Read, path::Path};

use byteorder::{ByteOrder, LittleEndian};
use memmap2::{Mmap, MmapOptions};
use zstd::{dict::DecoderDictionary, Decoder};

use crate::{map_lat_lon_to_index, HillshadeMetadata, LoadError, TileMetadata, HILLSHADE_FORMAT_VERSION};

mod ver3;
mod ver4;

pub struct Dataset {
	pub(crate) metadata: TileMetadata,
	pub(crate) tile_map: Vec<u64>,
	pub(crate) dictionary: DecoderDictionary<'static>,
	pub(crate) data: Mmap,
	pub(crate) data_offset: usize,
}

impl Dataset {
	pub(crate) const MAGIC: [u8; 5] = [115, 117, 115, 115, 121];
	pub(crate) const VER3_DICT_OFFSET: usize = Self::VER3_TILE_MAP_OFFSET + 360 * 180 * 8;
	pub(crate) const VER3_TILE_MAP_OFFSET: usize = 11;
	pub(crate) const VER4_DICT_OFFSET: usize = Self::VER4_TILE_MAP_OFFSET + 360 * 180 * 8;
	pub(crate) const VER4_TILE_MAP_OFFSET: usize = 13;

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

	pub fn get_raw_tile_data(&self, lat: i16, lon: i16) -> Option<Result<Vec<u8>, std::io::Error>> {
		tracy::zone!("Get Raw Tile");

		let index = map_lat_lon_to_index(lat, lon);
		let offset = self.tile_map[index] as usize;
		if offset == 0 {
			return None;
		}

		let frame = &self.data[offset - self.data_offset..];

		let res = self.metadata.resolution as usize;
		let mut decompressed = Vec::with_capacity(res * res * 2);
		decompressed.resize(decompressed.capacity(), 0);

		tracy::zone!("Decompress");

		let mut decoder = match Decoder::with_prepared_dictionary(frame, &self.dictionary) {
			Ok(x) => x,
			Err(e) => return Some(Err(e)),
		}
		.single_frame();
		match decoder.include_magicbytes(false) {
			Ok(x) => x,
			Err(e) => return Some(Err(e)),
		}
		match decoder.read_exact(&mut decompressed) {
			Ok(x) => x,
			Err(e) => return Some(Err(e)),
		}
		decoder.finish();

		Some(Ok(decompressed))
	}

	pub fn get_tile_and_compressed_size(
		&self, lat: i16, lon: i16,
	) -> Option<Result<(Vec<i16>, usize), std::io::Error>> {
		tracy::zone!("Get Tile");

		let index = map_lat_lon_to_index(lat, lon);
		let offset = self.tile_map[index] as usize;
		if offset == 0 {
			return None;
		}

		let frame = &self.data[offset - self.data_offset..];

		let res = self.metadata.resolution as usize;
		let mut decompressed = Vec::with_capacity(res * res * 2);
		decompressed.resize(decompressed.capacity(), 0);

		let remaining = {
			tracy::zone!("Decompress");

			let mut decoder = match Decoder::with_prepared_dictionary(frame, &self.dictionary) {
				Ok(x) => x,
				Err(e) => return Some(Err(e)),
			}
			.single_frame();
			match decoder.include_magicbytes(false) {
				Ok(x) => x,
				Err(e) => return Some(Err(e)),
			}
			match decoder.read_exact(&mut decompressed) {
				Ok(x) => x,
				Err(e) => return Some(Err(e)),
			}
			decoder.finish()
		};
		let compressed_size = frame.len() - remaining.len();

		let out = {
			tracy::zone!("Untile");
			let mut out = vec![0; (res * res) as usize];

			let tiling = self.metadata.tiling as usize;
			let tiles_per_row = res / tiling;
			for (i, tile) in decompressed.chunks_exact(tiling * tiling * 2).enumerate() {
				let tile_offset_x = (i % tiles_per_row) * tiling;
				let tile_offset_y = (i / tiles_per_row) * tiling;
				for (j, value) in tile.chunks_exact(2).enumerate() {
					let x = tile_offset_x + j % tiling;
					let y = tile_offset_y + j / tiling;
					let positive_height =
						u16::from_le_bytes(value.try_into().unwrap()) * self.metadata.height_resolution;
					out[y * res + x] = positive_height as i16 - 500;
				}
			}

			out
		};

		Some(Ok((out, compressed_size)))
	}

	pub fn get_tile_compressed_size(&self, lat: i16, lon: i16) -> usize {
		let index = map_lat_lon_to_index(lat, lon);
		let offset = self.tile_map[index] as usize;
		if offset == 0 {
			return 0;
		}

		let frame = &self.data[offset - self.data_offset..];
		Self::tile_frame_size(frame)
	}

	pub fn get_orphaned_tiles(&self, mut progress: impl FnMut(usize, usize)) -> Vec<(u64, u64)> {
		let mut ret = Vec::new();
		let mut offset = 0;

		let mut offsets: HashSet<_> = self.tile_map.iter().copied().filter(|&x| x != 0).collect();

		while !self.data[offset..].is_empty() {
			progress(offset, self.data.len());

			let data = &self.data[offset..];
			let tile_offset = (offset + self.data_offset) as u64;
			let size = Self::tile_frame_size(&data);

			if offsets.contains(&tile_offset) {
				offsets.remove(&tile_offset);
			} else {
				ret.push((offset as u64, size as u64));
			}

			offset += size;
		}

		ret
	}

	fn tile_frame_size(frame: &[u8]) -> usize {
		let header_descriptor = frame[0];
		let dict_id = header_descriptor & 0b11;
		let single_segment = (header_descriptor >> 5) & 1;
		let size_id = header_descriptor >> 6;
		let header_size = 1
			+ (1 - single_segment)
			+ match dict_id {
				0 => 0,
				1 => 1,
				2 => 2,
				3 => 4,
				_ => unreachable!(),
			} + match size_id {
			0 => single_segment,
			1 => 2,
			2 => 4,
			3 => 8,
			_ => unreachable!(),
		};

		let mut offset = header_size as usize;
		loop {
			let block = &frame[offset..];
			let header = LittleEndian::read_u24(&block[0..3]);

			let block_content_size = header >> 3;
			let last = header & 1;
			let block_ty = (header >> 1) & 0b11;

			let block_size = 3 + match block_ty {
				0 => block_content_size,
				1 => 1,
				2 => block_content_size,
				_ => unreachable!(),
			} as usize;

			offset += block_size;

			if last == 1 {
				break;
			}
		}

		offset
	}
}

pub struct Hillshade {
	pub(crate) metadata: HillshadeMetadata,
	pub(crate) tile_map: Vec<u64>,
	pub(crate) dictionary: DecoderDictionary<'static>,
	pub(crate) data: Mmap,
	pub(crate) data_offset: usize,
}

impl Hillshade {
	pub(crate) const DICT_OFFSET: usize = Self::TILE_MAP_OFFSET + 360 * 180 * 8;
	pub(crate) const MAGIC: [u8; 5] = [98, 117, 115, 115, 121];
	pub(crate) const TILE_MAP_OFFSET: usize = 11;

	pub fn load(dir: &Path) -> Result<Self, LoadError> {
		let meta = std::fs::metadata(&dir)?;
		if meta.is_dir() {
			Err(LoadError::UnsupportedFormatVersion)
		} else {
			let mut file = File::open(dir)?;
			let mut buffer = Vec::with_capacity(Self::DICT_OFFSET + 8); // max dict offset + 8
			buffer.resize(buffer.capacity(), 0);

			file.read_exact(&mut buffer[0..7])
				.map_err(|_| LoadError::InvalidFileSize)?;
			if buffer[0..5] != Self::MAGIC {
				return Err(LoadError::InvalidMagic);
			}
			let version = u16::from_le_bytes(buffer[5..7].try_into().unwrap());
			if version != HILLSHADE_FORMAT_VERSION {
				return Err(LoadError::UnsupportedFormatVersion);
			}

			file.read_exact(&mut buffer[7..Self::DICT_OFFSET + 8])
				.map_err(|_| LoadError::InvalidFileSize)?;
			let resolution = u16::from_le_bytes(buffer[7..9].try_into().unwrap());
			let tiling = u16::from_le_bytes(buffer[9..11].try_into().unwrap());
			let metadata = HillshadeMetadata {
				version: HILLSHADE_FORMAT_VERSION,
				resolution,
				tiling,
			};

			let tile_map = buffer[Self::TILE_MAP_OFFSET..Self::DICT_OFFSET]
				.chunks_exact(8)
				.map(|x| u64::from_le_bytes(x.try_into().unwrap()))
				.collect();
			let dict_size = u64::from_le_bytes(buffer[Self::DICT_OFFSET..Self::DICT_OFFSET + 8].try_into().unwrap());
			buffer.resize(dict_size as usize, 0);

			file.read_exact(&mut buffer).map_err(|_| LoadError::InvalidFileSize)?;
			let data_offset = Self::DICT_OFFSET + dict_size as usize + 8;

			Ok(Self {
				metadata,
				tile_map,
				dictionary: DecoderDictionary::copy(&buffer),
				data: unsafe { MmapOptions::new().offset(data_offset as _).map(&file)? },
				data_offset,
			})
		}
	}

	pub fn metadata(&self) -> HillshadeMetadata { self.metadata }

	pub fn tile_exists(&self, lat: i16, lon: i16) -> bool {
		let index = map_lat_lon_to_index(lat, lon);
		self.tile_map[index] != 0
	}

	pub fn tile_count(&self) -> usize { self.tile_map.iter().filter(|&&x| x != 0).count() }

	pub fn get_tile(&self, lat: i16, lon: i16) -> Option<Result<Vec<u8>, std::io::Error>> {
		self.get_tile_and_compressed_size(lat, lon).map(|x| x.map(|(x, _)| x))
	}

	pub fn get_tile_and_compressed_size(&self, lat: i16, lon: i16) -> Option<Result<(Vec<u8>, usize), std::io::Error>> {
		tracy::zone!("Get Tile");

		let index = map_lat_lon_to_index(lat, lon);
		let offset = self.tile_map[index] as usize;
		if offset == 0 {
			return None;
		}

		let frame = &self.data[offset - self.data_offset..];

		let res = self.metadata.resolution as usize;
		let mut decompressed = Vec::with_capacity(res * res * 2);
		decompressed.resize(decompressed.capacity(), 0);

		let remaining = {
			tracy::zone!("Decompress");

			let mut decoder = match Decoder::with_prepared_dictionary(frame, &self.dictionary) {
				Ok(x) => x,
				Err(e) => return Some(Err(e)),
			}
			.single_frame();
			match decoder.include_magicbytes(false) {
				Ok(x) => x,
				Err(e) => return Some(Err(e)),
			}
			match decoder.read_exact(&mut decompressed) {
				Ok(x) => x,
				Err(e) => return Some(Err(e)),
			}
			decoder.finish()
		};
		let compressed_size = frame.len() - remaining.len();

		let out = {
			tracy::zone!("Untile");
			let mut out = vec![0; (res * res) as usize];

			let tiling = self.metadata.tiling as usize;
			let tiles_per_row = res / tiling;
			for (i, tile) in decompressed.chunks_exact(tiling * tiling).enumerate() {
				let tile_offset_x = (i % tiles_per_row) * tiling;
				let tile_offset_y = (i / tiles_per_row) * tiling;
				for (j, &value) in tile.iter().enumerate() {
					let x = tile_offset_x + j % tiling;
					let y = tile_offset_y + j / tiling;
					out[y * res + x] = value;
				}
			}

			out
		};

		Some(Ok((out, compressed_size)))
	}
}
