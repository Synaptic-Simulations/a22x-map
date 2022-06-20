use std::{fs::File, io::Read};

use memmap2::MmapOptions;
use zstd::Decoder;

use crate::{map_lat_lon_to_index, Dataset, LoadError, TileMetadata};

pub fn load(buffer: &mut Vec<u8>, file: &mut File) -> Result<Dataset, LoadError> {
	file.read_exact(&mut buffer[7..Dataset::VER4_DICT_OFFSET + 8])
		.map_err(|_| LoadError::InvalidFileSize)?;
	let resolution = u16::from_le_bytes(buffer[7..9].try_into().unwrap());
	let height_resolution = u16::from_le_bytes(buffer[9..11].try_into().unwrap());
	let metadata = TileMetadata {
		version: 4,
		resolution,
		height_resolution,
		delta_compressed: false,
	};

	let tile_map = buffer[Dataset::VER4_TILE_MAP_OFFSET..Dataset::VER4_DICT_OFFSET]
		.chunks_exact(8)
		.map(|x| u64::from_le_bytes(x.try_into().unwrap()))
		.collect();
	let dict_size = u64::from_le_bytes(
		buffer[Dataset::VER4_DICT_OFFSET..Dataset::VER4_DICT_OFFSET + 8]
			.try_into()
			.unwrap(),
	);
	buffer.resize(dict_size as usize, 0);

	file.read_exact(buffer).map_err(|_| LoadError::InvalidFileSize)?;
	let data_offset = Dataset::VER4_DICT_OFFSET + dict_size as usize + 8;

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
	let mut decompressed = Vec::with_capacity(res * res * 2);
	decompressed.resize(decompressed.capacity(), 0);

	let remaining = {
		tracy::zone!("Decompress");

		let mut decoder = match Decoder::with_buffer(frame) {
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

	Some(Ok((
		decompressed
			.chunks_exact(2)
			.map(|value| {
				let positive_height = u16::from_le_bytes(value.try_into().unwrap()) * this.metadata.height_resolution;
				positive_height as i16 - 500
			})
			.collect(),
		compressed_size,
	)))
}
