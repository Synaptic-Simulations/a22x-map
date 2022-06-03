#![feature(int_log)]
//! A library for working with the `a22x` map's terrain format.

use std::{
	error::Error,
	fmt::{Debug, Display},
	fs::File,
	io::{BufReader, Read, Write},
	path::{Path, PathBuf},
	sync::{
		atomic::{AtomicU64, Ordering},
		RwLock,
	},
};

use memmap2::{Mmap, MmapOptions};
use zstd::{dict::DecoderDictionary, Decoder, Encoder};

use crate::old::GeoTile;

mod old;

/// ## Format version 1
/// Metadata file (_meta):
/// * [0..2]: The format version, little endian.
/// * [2..4]: The resolution of the square tile (one side).
/// * [4..6]: The resolution of height values (multiply with the raw value).
///
/// Heightmap file (N/S{lat}E/W{long}.geo):
/// * [0..2]: The minimum height in the tile, divided by `interval`.
/// * [2..3]: The number of bits used to encode the deltas of each height from the minimum.
/// * [3..]: The bit-packed heights, encoded as deltas from the minimum.
///
/// ## Format version 2
/// Heightmap files are LZ4 compressed.
///
/// ## Format version 3
/// Folders are replaced with files:
/// * [0..5]: Magic numbers: `[115, 117, 115, 115, 121]`.
/// * [5..7]: The format version, little endian.
/// * [7..9]: The resolution of the square tile (one side).
/// * [9..11]: The resolution of height values (multiply with the raw value).
/// * [11..11 + 360 * 180 * 8 @ tile_end]: 360 * 180 `u64`s that store the offsets of the tile in question (from the end
///   of the dictionary). If zero, the tile is not present.
/// * [tile_end..tile_end + 8]: The size of the decompression dictionary.
/// * [tile_end + 8..tile_end + 8 + decomp_dict_size]: The decompression dictionary.
/// * [tile_end + 8 + decomp_dict_size + offset...]: A zstd frame containing the compressed data of the tile, until the
///   next tile.
pub const FORMAT_VERSION: u16 = 3;

pub enum LoadError {
	InvalidFileSize,
	InvalidMagic,
	UnknownFormatVersion,
	Io(std::io::Error),
}

impl Display for LoadError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			LoadError::InvalidFileSize => write!(f, "Invalid file size"),
			Self::InvalidMagic => write!(f, "Invalid magic number"),
			Self::UnknownFormatVersion => write!(f, "Unknown format version"),
			Self::Io(x) => write!(f, "IO error: {}", x),
		}
	}
}

impl Debug for LoadError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Display::fmt(self, f) }
}

impl Error for LoadError {}

impl From<std::io::Error> for LoadError {
	fn from(x: std::io::Error) -> Self { Self::Io(x) }
}

pub enum Dataset {
	Ver1 {
		metadata: TileMetadata,
		path: PathBuf,
	},
	Ver3 {
		metadata: TileMetadata,
		tile_map: Vec<u64>,
		dictionary: DecoderDictionary<'static>,
		data: Mmap,
	},
}

impl Dataset {
	const DICT_START_OFFSET: usize = 11 + 360 * 180 * 8;
	const MAGIC: [u8; 5] = [115, 117, 115, 115, 121];
	const TILE_MAP_START_OFFSET: usize = 11;

	pub fn load(dir: impl Into<PathBuf>) -> Result<Self, LoadError> {
		let dir = dir.into();
		let meta = std::fs::metadata(&dir)?;
		if meta.is_dir() {
			Ok(Self::Ver1 {
				metadata: TileMetadata::load_from_directory(&dir)?,
				path: dir,
			})
		} else {
			let mut file = File::open(dir)?;
			let mut buffer = Vec::with_capacity(Self::DICT_START_OFFSET + 8);
			buffer.resize(buffer.capacity(), 0);
			file.read_exact(&mut buffer[0..7])
				.map_err(|_| LoadError::InvalidFileSize)?;

			if buffer[0..5] != Self::MAGIC {
				return Err(LoadError::InvalidMagic);
			}

			let version = u16::from_le_bytes(buffer[5..7].try_into().unwrap());
			if version != FORMAT_VERSION {
				return Err(LoadError::UnknownFormatVersion);
			}
			file.read_exact(&mut buffer[0..4])
				.map_err(|_| LoadError::InvalidFileSize)?;
			let resolution = u16::from_le_bytes(buffer[2..4].try_into().unwrap());
			let height_resolution = u16::from_le_bytes(buffer[4..6].try_into().unwrap());
			let metadata = TileMetadata {
				version,
				resolution,
				height_resolution,
			};

			file.read_exact(&mut buffer[0..Self::DICT_START_OFFSET - Self::TILE_MAP_START_OFFSET + 8])
				.map_err(|_| LoadError::InvalidFileSize)?;
			let tile_map = buffer[0..Self::DICT_START_OFFSET - Self::TILE_MAP_START_OFFSET]
				.chunks_exact(8)
				.map(|x| u64::from_le_bytes(x.try_into().unwrap()))
				.collect();

			let dict_size = u64::from_le_bytes(
				buffer[Self::DICT_START_OFFSET - Self::TILE_MAP_START_OFFSET
					..Self::DICT_START_OFFSET - Self::TILE_MAP_START_OFFSET + 8]
					.try_into()
					.unwrap(),
			);
			buffer.resize(dict_size as usize, 0);
			file.read_exact(&mut buffer[0..dict_size as usize])
				.map_err(|_| LoadError::InvalidFileSize)?;

			let offset = Self::DICT_START_OFFSET as u64 + dict_size;

			Ok(Self::Ver3 {
				metadata,
				tile_map,
				dictionary: DecoderDictionary::copy(&buffer),
				data: unsafe { MmapOptions::new().offset(offset).map(&file)? },
			})
		}
	}

	pub fn metadata(&self) -> TileMetadata {
		match self {
			Self::Ver1 { metadata, .. } => *metadata,
			Self::Ver3 { metadata, .. } => *metadata,
		}
	}

	pub fn get_tile(&self, lat: i16, lon: i16) -> Option<Vec<i16>> {
		match self {
			Self::Ver1 { metadata, path } => {
				let mut path = path.clone();
				GeoTile::get_file_name_for_coordinates(&mut path, lat, lon);
				GeoTile::load(metadata, &path).ok().map(|tile| tile.expand(metadata))
			},
			Self::Ver3 {
				tile_map,
				dictionary,
				data,
				metadata,
			} => {
				let index = map_lat_lon_to_index(lat, lon) * 8;
				let offset = tile_map[index] as usize;
				if offset == 0 {
					return None;
				}

				let frame = &data[offset..];

				let res = metadata.resolution as usize;
				let mut decompressed = Vec::with_capacity(res * res * 2);
				decompressed.resize(decompressed.capacity(), 0);

				let mut decoder = Decoder::with_prepared_dictionary(BufReader::new(frame), dictionary)
					.expect("Failed to create decoder")
					.single_frame();
				decoder.include_magicbytes(false).expect("Failed to set magic bytes");
				decoder.read_exact(&mut decompressed).expect("Failed to decompress");

				Some(
					decompressed
						.chunks_exact(2)
						.map(|x| {
							let positive_height =
								u16::from_le_bytes(x.try_into().unwrap()) * metadata.height_resolution;
							positive_height as i16 - 500
						})
						.collect(),
				)
			},
		}
	}

	pub fn builder(metadata: TileMetadata) -> DatasetBuilder { DatasetBuilder::new(metadata) }
}

pub struct DatasetBuilder {
	header: [u8; Dataset::TILE_MAP_START_OFFSET],
	tile_map: Vec<AtomicU64>,
	dictionary: Vec<u8>,
	data: RwLock<Vec<u8>>,
}

impl DatasetBuilder {
	pub fn new(metadata: TileMetadata) -> Self {
		assert_eq!(
			metadata.version, FORMAT_VERSION,
			"Can only build datasets with version {}",
			FORMAT_VERSION
		);

		let mut header = [0; Dataset::TILE_MAP_START_OFFSET];
		header[0..5].copy_from_slice(&Dataset::MAGIC);
		header[5..7].copy_from_slice(&metadata.version.to_le_bytes());
		header[7..9].copy_from_slice(&metadata.resolution.to_le_bytes());
		header[9..11].copy_from_slice(&metadata.height_resolution.to_le_bytes());

		DatasetBuilder {
			header,
			tile_map: std::iter::repeat_with(|| AtomicU64::new(0)).take(180 * 360).collect(),
			dictionary: Vec::new(),
			data: RwLock::new(Vec::new()),
		}
	}

	pub fn add_tile(&self, lat: i16, lon: i16, data: Vec<i16>) {
		let data: Vec<_> = data
			.iter()
			.flat_map(|x| {
				let positive_height = x + 500;
				let height = positive_height as f32 / u16::from_le_bytes(self.header[9..11].try_into().unwrap()) as f32;
				(height.round() as u16).to_le_bytes()
			})
			.collect();

		let mut temp = Vec::new();
		let mut encoder = Encoder::with_dictionary(&mut temp, 21, &self.dictionary).expect("Compression error");
		encoder.set_pledged_src_size(Some(data.len() as u64)).unwrap();
		encoder.include_magicbytes(false).unwrap();
		encoder.include_checksum(false).unwrap();
		encoder.long_distance_matching(true).unwrap();
		encoder.multithread(num_cpus::get() as _).unwrap();

		encoder.write_all(&data).unwrap();
		encoder.finish().unwrap();

		let index = map_lat_lon_to_index(lat, lon);
		let mut data = self.data.write().unwrap();
		let offset = data.len() as u64;
		data.extend(temp);
		self.tile_map[index].store(offset, Ordering::SeqCst);
	}

	pub fn finish(self, path: &Path) -> Result<(), std::io::Error> {
		let mut file = File::create(path)?;
		file.write_all(&self.header)?;
		file.write_all(unsafe { std::slice::from_raw_parts(self.tile_map.as_ptr() as _, self.tile_map.len() * 8) })?;
		file.write_all(&self.dictionary.len().to_le_bytes())?;
		file.write_all(&self.dictionary)?;
		file.write_all(&self.data.into_inner().unwrap())?;
		Ok(())
	}
}

pub fn map_lat_lon_to_index(lat: i16, lon: i16) -> usize {
	debug_assert!(lat >= -90 && lat < 90, "Latitude out of range");
	debug_assert!(lon >= -180 && lon < 180, "Longitude out of range");

	let lat = (lat + 90) as usize;
	let lon = (lon + 180) as usize;
	lat * 360 + lon
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct TileMetadata {
	/// The file format version.
	pub version: u16,
	/// The length of the side of the square tile.
	pub resolution: u16,
	/// The multiplier for the raw stored values.
	pub height_resolution: u16,
}
