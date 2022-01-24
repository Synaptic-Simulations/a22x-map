#![feature(int_log)]
//! A library for working with the `a22x` map's terrain format.

use bitpacking::{BitPacker, BitPacker8x};

/// ## Format version 1
///
/// `height` always refers to the altitude in meters MSL, in intervals of (divided by) 100m, with `0` being -500m MSL.
/// * [0..4]: The format version, little endian.
/// * [4..8]: The minimum height in the tile.
/// * [8..9]: The number of bits used to encode the deltas of each height from the minimum.
/// * [9..(512 * 512 * `bits` / 8)]: The heights, encoded as deltas from the minimum.
const FORMAT_VERSION: u32 = 1;

/// Compress a terrain tile to the map format.
///
/// The length of `data` must be exactly 512x512, with each element being the altitude in meters MSL.
pub fn compress(data: impl ExactSizeIterator<Item = i16>) -> Vec<u8> {
	assert_eq!(data.len(), 512 * 512, "data must be exactly 512x512");

	// Calculate the minimum height and range.
	let mut min = u32::MAX;
	let mut max = u32::MIN;
	// The format's `height`.
	let mut data: Vec<_> = data
		.map(|x| {
			let altitude_by_100 = x as f32 / 100.0;
			let value = (altitude_by_100 + 5.0).round() as u32;

			min = min.min(value);
			max = max.max(value);

			value
		})
		.collect();

	// Calculate deltas
	for x in data.iter_mut() {
		*x -= min;
	}

	// The max number of bits used to encode the deltas of each height from the minimum.
	let bits = ((max - min).log2() + 1) as u8;

	let block_size = BitPacker8x::compressed_block_size(bits);
	let mut out = vec![0; 10 + 512 * 2 * block_size];

	out[0..4].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
	out[4..8].copy_from_slice(&min.to_le_bytes());
	out[8..9].copy_from_slice(&bits.to_le_bytes());

	let bitpacker = BitPacker8x::new();

	for (i, chunk) in data.chunks(256).enumerate() {
		bitpacker.compress(chunk, &mut out[(9 + block_size * i)..], bits);
	}

	out
}

/// Decompress a terrain tile from the map format.
pub fn decompress(data: &[u8]) -> Vec<i16> {
	let format_version = u32::from_le_bytes(data[0..4].try_into().unwrap());
	assert_eq!(format_version, FORMAT_VERSION, "invalid format version");

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
