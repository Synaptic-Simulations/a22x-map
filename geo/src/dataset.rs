use std::{fs::File, io::Read, path::Path};

use hcomp::decode::decode;
use libwebp_sys::WebPDecodeRGBAInto;
use memmap2::{Mmap, MmapOptions};

use crate::{map_lat_lon_to_index, LoadError, TileMetadata, FORMAT_VERSION};

pub struct Dataset {
	pub(crate) metadata: TileMetadata,
	pub(crate) tile_map: Vec<u64>,
	pub(crate) data: Mmap,
}

impl Dataset {
	pub(crate) const MAGIC: [u8; 5] = [115, 117, 115, 115, 121];

	pub fn load(dir: &Path) -> Result<Self, LoadError> {
		let meta = std::fs::metadata(&dir)?;
		if meta.is_dir() {
			Err(LoadError::UnsupportedFormatVersion)
		} else {
			let mut file = File::open(dir)?;
			let mut buffer = Vec::with_capacity(32 + 360 * 180 * 8);
			buffer.resize(buffer.capacity(), 0);

			file.read_exact(&mut buffer).map_err(|_| LoadError::InvalidFileSize)?;

			if buffer[0..5] != Self::MAGIC {
				return Err(LoadError::InvalidMagic);
			}
			let version = u16::from_le_bytes(buffer[5..7].try_into().unwrap());
			if version != FORMAT_VERSION {
				return Err(LoadError::UnsupportedFormatVersion);
			}
			let resolution = u16::from_le_bytes(buffer[7..9].try_into().unwrap());
			let height_resolution = u16::from_le_bytes(buffer[9..11].try_into().unwrap());
			let metadata = TileMetadata {
				version: FORMAT_VERSION,
				resolution,
				height_resolution,
			};

			let tile_map = buffer[32..]
				.chunks_exact(8)
				.map(|x| u64::from_le_bytes(x.try_into().unwrap()))
				.collect();

			Ok(Dataset {
				metadata,
				tile_map,
				data: unsafe { MmapOptions::new().offset(buffer.len() as _).map(&file)? },
			})
		}
	}

	pub fn metadata(&self) -> TileMetadata { self.metadata }

	pub fn tile_exists(&self, lat: i16, lon: i16) -> bool {
		let index = map_lat_lon_to_index(lat, lon);
		self.tile_map[index] != 0
	}

	pub fn tile_count(&self) -> usize { self.tile_map.iter().filter(|&&x| x != 0).count() }

	pub fn get_tile(&self, lat: i16, lon: i16) -> Option<Result<(Vec<u16>, Vec<u8>), std::io::Error>> {
		Some(match self.get_full_tile(lat, lon)? {
			Ok((mut data, water, hillshade)) => {
				for (h, w) in data.iter_mut().zip(water) {
					*h |= (w as u16) << 15;
				}

				Ok((data, hillshade))
			},
			Err(e) => Err(e),
		})
	}

	pub fn get_full_tile(&self, lat: i16, lon: i16) -> Option<Result<(Vec<u16>, Vec<u8>, Vec<u8>), std::io::Error>> {
		tracy::zone!("Get Tile");

		let index = map_lat_lon_to_index(lat, lon);
		let offset = self.tile_map[index] as usize;
		if offset == 0 {
			return None;
		}

		let frame = &self.data[offset - (32 + 360 * 180 * 8)..];
		let res = self.metadata.resolution as u32;

		let (data, len) = {
			tracy::zone!("Decompress height");
			match decode(frame, res, res) {
				Ok(x) => x,
				Err(e) => return Some(Err(e)),
			}
		};
		let data: Vec<_> = {
			tracy::zone!("Unmap height");
			data.data
				.into_owned()
				.into_iter()
				.map(|x| x * self.metadata.height_resolution)
				.collect()
		};
		let (water, rest) = {
			tracy::zone!("Decompress water");

			match Self::decompress_u8_webp(&frame[len..], res, res) {
				Ok(x) => x,
				Err(e) => return Some(Err(e)),
			}
		};
		let (hillshade, _) = {
			tracy::zone!("Decompress hillshade");
			match Self::decompress_u8_webp(rest, res, res) {
				Ok(x) => x,
				Err(e) => return Some(Err(e)),
			}
		};

		Some(Ok((data, water, hillshade)))
	}

	fn decompress_u8_webp(data: &[u8], width: u32, height: u32) -> Result<(Vec<u8>, &[u8]), std::io::Error> {
		unsafe {
			let frame_size = u32::from_le_bytes(data[4..8].try_into().unwrap()) + 8;
			let frame = &data[..frame_size as usize];
			let mut decompressed = vec![0; width as usize * height as usize];
			if WebPDecodeRGBAInto(
				frame.as_ptr(),
				frame.len(),
				decompressed.as_mut_ptr(),
				decompressed.len(),
				width as i32 * 2,
			)
			.is_null()
			{
				return Err(std::io::Error::new(
					std::io::ErrorKind::Other,
					"WebPDecodeRGBAInto failed",
				));
			}

			Ok((decompressed, &data[frame_size as usize..]))
		}
	}
}
