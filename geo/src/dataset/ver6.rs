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
	let decompressed = match decompress(res, frame) {
		Ok(x) => x,
		Err(e) => return Some(Err(e)),
	};
	let unpaletted = unpalette(res, decompressed);

	Some(Ok((x, compressed_size)))
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
			.chain(decompressed[4..].map(|x| if *x == 0 { 0 } else { *x + min - 1 }))
			.collect()
	}
}
