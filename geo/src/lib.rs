#![feature(int_log)]
//! A library for working with the `a22x` map's terrain format.

use std::{
	error::Error,
	fmt::{Debug, Display},
	fs::File,
	io::Write,
	path::Path,
};

use bitpacking::{BitPacker, BitPacker8x};
pub use lz4;
use lz4::EncoderBuilder;

/// ## Format version 1
/// Metadata file (map.meta):
/// * [0..2]: The format version, little endian.
/// * [2..4]: The resolution of the square tile (one side).
/// * [4..6]: The resolution of height values (multiply with the raw value).
///
/// Heightmap file (N/S{lat}E/W{long}.geo):
/// * [0..2]: The minimum height in the tile, divided by `interval`.
/// * [2..3]: The number of bits used to encode the deltas of each height from the minimum.
/// * [3..]: The bit-packed heights, encoded as deltas from the minimum.
/// The files are also LZ4 compressed.
pub const FORMAT_VERSION: u16 = 1;

pub enum DecompressError {
	UnknownFormatVersion,
}

pub struct TileMetadata {
	/// The file format version.
	pub version: u16,
	/// The length of the side of the square tile.
	pub resolution: u16,
	/// The multiplier for the raw stored values.
	pub height_resolution: u16,
}

impl TileMetadata {
	pub fn write_to_directory(&self, dir: &Path) -> Result<(), std::io::Error> {
		let mut path = dir.to_path_buf();
		path.push("map.meta");

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

pub struct GeoTile {
	min_height: u16,
	bits: u8,
	data: Vec<u8>,
}

impl GeoTile {
	/// Compress a terrain tile from raw data.
	///
	/// `data` must have length `metadata.resolution^2`.
	pub fn new(metadata: &TileMetadata, data: impl IntoIterator<Item = i16>) -> Result<Self, CompressError> {
		assert_eq!(
			metadata.version, FORMAT_VERSION,
			"Compressing is only supported for the latest format version ({})",
			FORMAT_VERSION
		);

		if (metadata.resolution as usize * metadata.resolution as usize) % 256 != 0 {
			return Err(CompressError::UnsupportedResolution(metadata.resolution));
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
				min_height: min,
				bits: 0,
				data: vec![],
			}
		} else {
			// The max number of bits used to encode the deltas of each height from the minimum.
			let bits = ((max - min).log2() + 1) as u8;

			let block_size = BitPacker8x::compressed_block_size(bits);
			let block_count = (metadata.resolution as usize * metadata.resolution as usize) / 256;
			let mut out = vec![0; block_count * block_size];

			let packer = BitPacker8x::new();
			for (i, chunk) in data.chunks(256).enumerate() {
				packer.compress(chunk, &mut out[(block_size * i)..], bits);
			}

			Self {
				min_height: min,
				bits,
				data: out,
			}
		})
	}

	pub fn write_to_directory(&self, dir: &Path, latitude: i16, longitude: i16) -> Result<(), std::io::Error> {
		let mut path = dir.to_path_buf();
		path.push(format!(
			"{}{}{}{}.geo",
			if latitude < 0 { 'S' } else { 'N' },
			latitude.abs(),
			if longitude < 0 { 'W' } else { 'E' },
			longitude.abs()
		));

		let mut file = EncoderBuilder::new().level(9).build(File::create(path)?)?;
		file.write_all(&self.min_height.to_le_bytes())?;
		file.write_all(&self.bits.to_le_bytes())?;
		file.write_all(&self.data)?;
		file.finish().1
	}
}

/// Decompress a terrain tile from the map format.
pub fn decompress(data: &[u8]) -> Vec<i16> {
	let format_version = u32::from_le_bytes(data[0..4].try_into().unwrap());
	// assert_eq!(format_version, FORMAT_VERSION, "invalid format version");

	let min = u32::from_le_bytes(data[4..8].try_into().unwrap());
	let bits = u8::from_le_bytes(data[8..9].try_into().unwrap());

	let block_size = BitPacker8x::compressed_block_size(bits);

	let mut out = vec![0; 512 * 512];

	let bitpacker = BitPacker8x::new();

	for (i, chunk) in out.chunks_mut(256).enumerate() {
		bitpacker.decompress(&data[(9 + block_size * i)..], chunk, bits);
	}

	out.into_iter().map(|x| ((x + min) as i16 - 5) * 100).collect()
}
