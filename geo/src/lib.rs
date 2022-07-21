//! A library for working with the `a22x` map's terrain format.

extern crate core;

use std::{
	error::Error,
	fmt::{Debug, Display},
};

mod dataset;
pub use dataset::*;
mod builder;
pub use builder::*;

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
/// Format versions 1 and 2 are unsupported.
///
/// ## Format version 3
/// Everything is little-endian.
/// There is a single file:
/// * [0..5]: Magic number: `[115, 117, 115, 115, 121]`.
/// * [5..7]: The format version, little endian.
/// * [7..9]: The resolution of the square tile (one side).
/// * [9..11]: The resolution of height values (multiply with the raw value).
/// * [11..11 + 360 * 180 * 8 @ tile_end]: 360 * 180 `u64`s that store the offsets of the tile in question (from the
///   beginning of the file). If zero, the tile is not present.
/// * [tile_end..tile_end + 8]: The size of the decompression dictionary.
/// * [tile_end + 8..tile_end + 8 + decomp_dict_size]: The decompression dictionary.
/// * [tile_end + 8 + decomp_dict_size + offset...]: A zstd frame containing the compressed data of the tile, until the
///   next tile.
///
/// Each tile is laid out in row-major order. The origin (lowest latitude and longitude) is the bottom-left.
/// A special height value of `-500` indicates that the pixel is covered by water.
///
/// # Format version 4
/// Largely the same as version 3, but tiles the data in each tile.
/// * [0..5]: Magic number: `[115, 117, 115, 115, 121]`.
/// * [5..7]: The format version, little endian.
/// * [7..9]: The resolution of the square tile (one side).
/// * [9..11]: The resolution of height values (multiply with the raw value).
/// * [11..13]: The size of each mini-tile.
/// * [13..13 + 360 * 180 * 8 @ tile_end]: 360 * 180 `u64`s that store the offsets of the tile in question (from the
///   beginning of the file). If zero, the tile is not present.
/// * [tile_end..tile_end + 8] @ decomp_dict_size: The size of the decompression dictionary.
/// * [tile_end + 8..tile_end + 8 + decomp_dict_size]: The decompression dictionary.
/// * [tile_end + 8 + decomp_dict_size + offset...]: A zstd frame containing the compressed data of the tile, until the
///   next tile.
///
/// # Format version 5
/// * [0..5]: Magic number: `[115, 117, 115, 115, 121]`.
/// * [5..7]: The format version, little endian.
/// * [7..9]: The resolution of the square tile (one side).
/// * [9..11]: The resolution of height values (multiply with the raw value).
/// * [11..12]: If the data in each tile is delta-compressed.
/// * [12..12 + 360 * 180 * 8] @ offsets: 360 * 180 `u64`s that store the offsets of the tile in question (from the
///   beginning of the file). If zero, the tile is not present.
/// * [offset..]: A webp image containing the compressed data of the tile, until the next tile.
///
/// Image specifics:
/// * If the data is delta-compressed, the first pixel is the minimum (mapped) height, and each following pixel is the
///   delta from the previous.
/// * Otherwise, every pixel is the mapped height.
/// * Since webp only supports rgb8 or rgba8, we store the data as a webp image of resolution `res / 2` * `tile_size`,
///   with each `u16` pixel splatted over two components of an rgba8 pixel.
///
/// # Format version 6
/// * [0..5]: Magic number: `[115, 117, 115, 115, 121]`.
/// * [5..7]: The format version, little endian.
/// * [7..9]: The resolution of the square tile (one side).
/// * [9..11]: The resolution of height values (round each raw value to the nearest multiple).
/// * [11]: Empty space.
/// * [12..12 + 360 * 180 * 8] @ offsets: 360 * 180 `u64`s that store the offsets of the tile in question (from the
///   beginning of the file). If zero, the tile is not present.
/// * [offset..]: A zstd frame containing the compressed data of the tile, until the next tile.
///
/// ## Input to zstd
/// Each tile is laid out in row-major order. The origin (lowest latitude and longitude) is the bottom-left.
/// A special height value of `-500` indicates that the pixel is covered by water.
///
/// Before submitting the data to zstd, a series of transformations are applied, each using the input of the former.
///
/// ### Heightmapping
/// The height values are downsampled and converted to an unsigned 16 bit integer.
/// 1. 500 is added to each height value, making 0 signify a water pixel (since lowest point on Earth is -431m).
/// 2. The values are divided by the height resolution, and rounded to the nearest integer.
///
/// ### Spatial prediction
/// The top-left pixel of each tile contains the raw value from the previous transform.
/// The pixel directly to the right and bottom store the deltas from this pixel.
/// The first row and column store deltas from a linear predictor (which tries to keep dhdx/dhdy constant).
/// The remaining pixels store deltas from a plane predictor, which assumes the point is on the same plane as its
/// neighbours to the left, top, and top-left.
///
/// Since deltas can be both positive and negative, and water pixels have a fixed magic height value, the deltas are
/// also transformed. We assume that the largest possible delta in a pixel is 7000m, so we store the deltas as unsigned
/// integers, where 7000m indicates 0 delta. We also have a special value of 0 for any delta that leads to a water
/// pixel.
///
/// ### Paletting
/// If there are 256 or values (not including the first pixel), a palette is used, with each other pixel referring to a
/// value stored in the palette.
///
/// If variance in the palette or prediction deltas can fit within a byte, all the 16 bit values are compressed into 1
/// byte (0 now represents water), with an offset from the minimum.
///
/// # Format version 7
/// Deprecate all old versions.
/// * [0..5]: Magic number: `[115, 117, 115, 115, 121]`.
/// * [5..7]: The format version, little endian.
/// * [7..9]: The resolution of the square tile (one side).
/// * [9..11]: The resolution of height values (round each raw value to the nearest multiple).
/// * [11..32]: Empty space, for future use. Must be 0.
/// * [32..32 + 360 * 180 * 8] @ offsets: 360 * 180 `u64`s that store the offsets of the tile in question (from the
///   beginning of the file). If zero, the tile is not present.
/// * [offset..]: A hcomp frame containing the compressed data of the tile, until the next tile, followed by a webp
///   image of the water mask.
///
/// # Format version 8
/// Deprecate all old versions.
/// * [0..5]: Magic number: `[115, 117, 115, 115, 121]`.
/// * [5..7]: The format version, little endian.
/// * [7..9]: The resolution of the square tile (one side).
/// * [9..11]: The resolution of height values (round each raw value to the nearest multiple).
/// * [11..32]: Empty space, for future use. Must be 0.
/// * [32..32 + 360 * 180 * 8] @ offsets: 360 * 180 `u64`s that store the offsets of the tile in question (from the
///   beginning of the file). If zero, the tile is not present.
/// * [offset..]: A hcomp frame containing the compressed data of the tile, until the next tile, followed by a webp
///   image of the water mask, further followed by a webp image of the hillshade.
pub const FORMAT_VERSION: u16 = 8;

pub enum LoadError {
	InvalidFileSize,
	InvalidMagic,
	UnsupportedFormatVersion,
	Io(std::io::Error),
}

impl Display for LoadError {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			Self::InvalidFileSize => write!(f, "Invalid file size"),
			Self::InvalidMagic => write!(f, "Invalid magic number"),
			Self::UnsupportedFormatVersion => write!(f, "Unknown format version"),
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

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct TileMetadata {
	/// The file format version.
	pub version: u16,
	/// The length of the side of the square tile.
	pub resolution: u16,
	/// The multiplier for the raw stored values.
	pub height_resolution: u16,
}

pub fn map_lat_lon_to_index(lat: i16, lon: i16) -> usize {
	debug_assert!(lat >= -90 && lat < 90, "Latitude out of range");
	debug_assert!(lon >= -180 && lon < 180, "Longitude out of range");

	let lat = (lat + 90) as usize;
	let lon = (lon + 180) as usize;
	lat * 360 + lon
}

pub fn map_index_to_lat_lon(index: usize) -> (i16, i16) {
	debug_assert!(index < 180 * 360, "Index out of range");

	let lat = (index / 360) as i16 - 90;
	let lon = (index % 360) as i16 - 180;
	(lat, lon)
}
