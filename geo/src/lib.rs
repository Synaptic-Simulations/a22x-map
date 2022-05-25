#![feature(int_log)]
//! A library for working with the `a22x` map's terrain format.

use std::{
	error::Error,
	fmt::{Debug, Display},
	fs::File,
	io::{Read, Write},
	path::{Path, PathBuf},
};

use bitpacking::{BitPacker, BitPacker8x};
pub use lz4;
use lz4::EncoderBuilder;

mod load;

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
pub const FORMAT_VERSION: u16 = 2;

pub struct TileMetadata {
	/// The file format version.
	pub version: u16,
	/// The length of the side of the square tile.
	pub resolution: u16,
	/// The multiplier for the raw stored values.
	pub height_resolution: u16,
}

impl TileMetadata {
	pub fn load_from_directory(dir: &Path) -> Result<Self, std::io::Error> {
		let mut path = dir.to_path_buf();
		path.push("_meta");

		let mut file = File::open(path)?;
		let mut buf = Vec::new();
		file.read_to_end(&mut buf)?;

		let version = u16::from_le_bytes(buf[0..2].try_into().unwrap());
		let resolution = u16::from_le_bytes(buf[2..4].try_into().unwrap());
		let height_resolution = u16::from_le_bytes(buf[4..6].try_into().unwrap());

		Ok(Self {
			version,
			resolution,
			height_resolution,
		})
	}

	pub fn write_to_directory(&self, dir: &Path) -> Result<(), std::io::Error> {
		let mut path = dir.to_path_buf();
		path.push("_meta");

		let mut file = File::create(path)?;
		file.write_all(&self.version.to_le_bytes())?;
		file.write_all(&self.resolution.to_le_bytes())?;
		file.write_all(&self.height_resolution.to_le_bytes())?;

		Ok(())
	}
}

pub enum CompressError {
	UnsupportedResolution(u16),
}

impl Display for CompressError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			Self::UnsupportedResolution(x) => write!(f, "Unsupported resolution: {} (must be a multiple of 256)", x),
		}
	}
}

impl Debug for CompressError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Display::fmt(self, f) }
}

impl Error for CompressError {}

pub enum LoadError {
	UnknownFormatVersion,
	Io(std::io::Error),
}

impl Display for LoadError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
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

#[derive(Clone)]
pub struct GeoTile {
	data: Vec<u8>,
}

impl GeoTile {
	pub fn get_file_name_for_coordinates(buf: &mut PathBuf, lat: i16, lon: i16) {
		buf.push(format!(
			"{}{}{}{}.geo",
			if lat < 0 { 'S' } else { 'N' },
			lat.abs(),
			if lon < 0 { 'W' } else { 'E' },
			lon.abs()
		));
	}

	pub fn get_coordinates_from_file_name(path: &Path) -> (i16, i16) {
		let file_name = path.file_name().unwrap().to_str().unwrap();
		let e_or_w_location = file_name.find(['E', 'W']).unwrap();
		let mut lat: i16 = file_name[1..e_or_w_location].parse().unwrap();
		if &file_name[0..1] == "S" {
			lat = -lat;
		}
		let mut lon: i16 = file_name[e_or_w_location + 1..file_name.len() - 4].parse().unwrap();
		if &file_name[e_or_w_location..e_or_w_location + 1] == "W" {
			lon = -lon;
		}

		(lat, lon)
	}

	/// Compress a terrain tile from raw data.
	///
	/// `data` must have length `metadata.resolution^2`.
	pub fn new(metadata: &TileMetadata, data: impl IntoIterator<Item = i16>) -> Result<Self, CompressError> {
		assert_eq!(
			metadata.version, FORMAT_VERSION,
			"Compressing is only supported for the latest format version ({})",
			FORMAT_VERSION
		);

		if (metadata.resolution as usize * metadata.resolution as usize) % BitPacker8x::BLOCK_LEN != 0 {
			return Err(CompressError::UnsupportedResolution(BitPacker8x::BLOCK_LEN as _));
		}

		// Calculate the minimum height and range.
		let mut min = u16::MAX;
		let mut max = u16::MIN;
		// The format's `height`.
		let mut data: Vec<_> = data
			.into_iter()
			.map(|raw| {
				let positive_altitude = raw + 500; // Lowest altitude is -414m.
				let height = positive_altitude as f32 / metadata.height_resolution as f32;
				let height = height.round() as u16;
				min = min.min(height);
				max = max.max(height);
				height as u32
			})
			.collect();

		// Calculate deltas
		for x in data.iter_mut() {
			*x -= min as u32;
		}

		Ok(if max == min {
			Self {
				data: {
					let mut vec = vec![0; 3];
					vec[0..2].copy_from_slice(&min.to_le_bytes());
					vec[2] = 0;
					vec
				},
			}
		} else {
			// The max number of bits used to encode the deltas of each height from the minimum.
			let bits = ((max - min).log2() + 1) as u8;

			let block_size = BitPacker8x::compressed_block_size(bits);
			let block_count = (metadata.resolution as usize * metadata.resolution as usize) / BitPacker8x::BLOCK_LEN;

			let mut out = vec![0; 3 + block_count * block_size];
			out[0..2].copy_from_slice(&min.to_le_bytes());
			out[2] = bits;

			let packer = BitPacker8x::new();
			for (i, chunk) in data.chunks(BitPacker8x::BLOCK_LEN).enumerate() {
				packer.compress(chunk, &mut out[block_size * i..], bits);
			}

			Self { data: out }
		})
	}

	pub fn load(metadata: &TileMetadata, path: &Path) -> Result<Self, LoadError> {
		match metadata.version {
			1 => load::load_v1(path),
			2 => load::load_v2(path),
			_ => Err(LoadError::UnknownFormatVersion),
		}
	}

	pub fn chunk(&self) -> &[u8] { &self.data }

	pub fn expand(&self, metadata: &TileMetadata) -> Vec<i16> {
		let min_height = u16::from_le_bytes(self.data[0..2].try_into().unwrap());
		let bits = self.data[2];

		if bits == 0 {
			let positive_height = min_height * metadata.height_resolution;
			let height = positive_height as i16 - 500; // Lowest altitude is -414m.
			vec![height; metadata.resolution as usize * metadata.resolution as usize]
		} else {
			let mut out = vec![0; metadata.resolution as usize * metadata.resolution as usize];
			let block_size = BitPacker8x::compressed_block_size(bits);
			let packer = BitPacker8x::new();
			for (i, chunk) in out.chunks_exact_mut(BitPacker8x::BLOCK_LEN).enumerate() {
				let compressed = &self.data[3 + block_size * i..];
				packer.decompress(compressed, chunk, bits);
			}

			out.into_iter()
				.map(|height| {
					let positive_height = (height as u16 + min_height) * metadata.height_resolution;
					positive_height as i16 - 500 // Lowest altitude is -414m.
				})
				.collect()
		}
	}

	pub fn write_to_directory(&self, dir: &Path, latitude: i16, longitude: i16) -> Result<(), std::io::Error> {
		let mut path = dir.to_path_buf();
		Self::get_file_name_for_coordinates(&mut path, latitude, longitude);

		let mut file = EncoderBuilder::new().level(9).build(File::create(path)?)?;
		file.write_all(&self.data)?;
		file.finish().1
	}
}
