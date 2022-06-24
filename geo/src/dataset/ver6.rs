use std::{fs::File, io::Read};

use memmap2::MmapOptions;
use zstd::Decoder;

use crate::{map_lat_lon_to_index, Dataset, LoadError, TileMetadata};

pub fn load(buffer: &mut Vec<u8>, file: &mut File) -> Result<Dataset, LoadError> {
	file.read_exact(&mut buffer[7..Dataset::VER5_TILE_MAP_OFFSET + 360 * 180 * 8])
		.map_err(|_| LoadError::InvalidFileSize)?;
	let resolution = u16::from_le_bytes(buffer[7..9].try_into().unwrap());
	let height_resolution = u16::from_le_bytes(buffer[9..11].try_into().unwrap());
	let metadata = TileMetadata {
		version: 6,
		resolution,
		height_resolution,
	};

	let tile_map = buffer[Dataset::VER5_TILE_MAP_OFFSET..Dataset::VER5_TILE_MAP_OFFSET + 360 * 180 * 8]
		.chunks_exact(8)
		.map(|x| u64::from_le_bytes(x.try_into().unwrap()))
		.collect();
	let data_offset = Dataset::VER5_TILE_MAP_OFFSET + 360 * 180 * 8;

	Ok(Dataset {
		metadata,
		tile_map,
		data: unsafe { MmapOptions::new().offset(data_offset as _).map(&*file)? },
		data_offset,
	})
}

pub fn get_tile(this: &Dataset, lat: i16, lon: i16) -> Option<Result<(Vec<i16>, usize), std::io::Error>> {
	let index = map_lat_lon_to_index(lat, lon);
	let offset = this.tile_map[index] as usize;
	if offset == 0 {
		return None;
	}

	let frame = &this.data[offset - this.data_offset..];

	let res = this.metadata.resolution as usize;
	let (decompressed, size) = match decompress(res, frame) {
		Ok(x) => x,
		Err(e) => return Some(Err(e)),
	};
	let unpaletted = unpalette(res, decompressed);
	let unpredicted = unpredict(res, unpaletted);
	let output = unmap(this.metadata.height_resolution, unpredicted);

	Some(Ok((output, size)))
}

fn decompress(res: usize, data: &[u8]) -> Result<(Vec<u8>, usize), std::io::Error> {
	tracy::zone!("Decompress");

	let mut decoder = Decoder::with_buffer(data)?.single_frame();
	decoder.include_magicbytes(false)?;
	decoder.window_log_max(24)?;
	let mut decompressed = Vec::with_capacity(res * res * 2);
	decoder.read_to_end(&mut decompressed)?;
	let remaining = decoder.finish();
	Ok((decompressed, data.len() - remaining.len()))
}

fn unpalette(res: usize, decompressed: Vec<u8>) -> Vec<u16> {
	tracy::zone!("Unpalette");

	let raw_size = res * res * 2 + 2;
	let raw_byte_size = res * res + 3;
	if decompressed.len() == raw_size {
		let min = u16::from_le_bytes(decompressed[0..2].try_into().unwrap());
		decompressed[2..]
			.chunks_exact(2)
			.map(|x| u16::from_le_bytes(x.try_into().unwrap()) + min)
			.collect()
	} else if decompressed.len() == raw_byte_size {
		let min = u16::from_le_bytes(decompressed[0..2].try_into().unwrap());
		let first = u16::from_le_bytes(decompressed[2..4].try_into().unwrap());
		std::iter::once(first)
			.chain(
				decompressed[4..]
					.iter()
					.map(|x| if *x == 0 { 0 } else { *x as u16 - 1 + min }),
			)
			.collect()
	} else {
		let palette_len = decompressed[0] as usize + 1;
		let raw_len = decompressed.len() - (res * res + 2);
		if raw_len == palette_len * 2 {
			let end = palette_len * 2 + 1;
			let palette = &decompressed[1..end];
			let mut palette = std::iter::once(u16::from_le_bytes(palette[0..2].try_into().unwrap()))
				.chain(
					palette[2..]
						.chunks_exact(2)
						.map(|x| u16::from_le_bytes(x.try_into().unwrap()) + 1),
				)
				.collect::<Vec<_>>();
			for offset in 1..palette.len() {
				palette[offset] += palette[offset - 1];
			}
			std::iter::once(u16::from_le_bytes(decompressed[end..end + 2].try_into().unwrap()))
				.chain(decompressed[end + 2..].iter().map(|x| palette[*x as usize]))
				.collect()
		} else if raw_len == palette_len + 1 {
			let end = palette_len + 2;
			let palette = &decompressed[1..end];
			let mut palette = std::iter::once(u16::from_le_bytes(palette[0..2].try_into().unwrap()))
				.chain(palette[2..].iter().map(|x| *x as u16 + 1))
				.collect::<Vec<_>>();
			for offset in 1..palette.len() {
				palette[offset] += palette[offset - 1];
			}
			std::iter::once(u16::from_le_bytes(decompressed[end..end + 2].try_into().unwrap()))
				.chain(decompressed[end + 2..].iter().map(|x| palette[*x as usize]))
				.collect()
		} else {
			unreachable!("Invalid raw data length");
		}
	}
}

fn unpredict(res: usize, mut unpaletted: Vec<u16>) -> Vec<u16> {
	tracy::zone!("Unpredict");

	fn delta(current: u16, out: u16) -> u16 {
		if out == 0 {
			0
		} else {
			let delta = out as i32 - 7000;
			(current as i32 + delta) as u16
		}
	}

	fn linear(previous: u16, current: u16, out: u16) -> u16 {
		if out == 0 {
			0
		} else {
			let delta = current as i32 - previous as i32;
			let pred = current as i32 + delta;
			let delta = out as i32 - 7000;
			(pred + delta) as u16
		}
	}

	fn plane(left: u16, top: u16, top_left: u16, out: u16) -> u16 {
		if out == 0 {
			0
		} else {
			let dhdy = left as i32 - top_left as i32;
			let pred = top as i32 + dhdy;
			let delta = out as i32 - 7000;
			(pred + delta) as u16
		}
	}

	// (1, 0) and (0, 1).
	unpaletted[1] = delta(unpaletted[0], unpaletted[1]);
	unpaletted[res] = delta(unpaletted[0], unpaletted[res]);
	// First row and column.
	for x in 2..res {
		let previous = unpaletted[x - 2];
		let current = unpaletted[x - 1];
		let out = unpaletted[x];
		unpaletted[x] = linear(previous, current, out);
	}
	for y in 2..res {
		let previous = unpaletted[(y - 2) * res];
		let current = unpaletted[(y - 1) * res];
		let out = unpaletted[y * res];
		unpaletted[y * res] = linear(previous, current, out);
	}
	// The rest.
	for x in 1..res {
		for y in 1..res {
			let left = unpaletted[y * res + x - 1];
			let top = unpaletted[(y - 1) * res + x];
			let top_left = unpaletted[(y - 1) * res + x - 1];
			let out = unpaletted[y * res + x];
			unpaletted[y * res + x] = plane(left, top, top_left, out);
		}
	}

	unpaletted
}

fn unmap(height_res: u16, unpredicted: Vec<u16>) -> Vec<i16> {
	tracy::zone!("Unmap");

	unpredicted
		.into_iter()
		.map(|h| {
			let h = h * height_res;
			(h as i32 - 500) as i16
		})
		.collect()
}
