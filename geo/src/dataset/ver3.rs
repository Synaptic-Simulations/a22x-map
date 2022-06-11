use std::{fs::File, io::Read};

use memmap2::MmapOptions;
use zstd::dict::DecoderDictionary;

use crate::{Dataset, LoadError, TileMetadata};

pub fn load(buffer: &mut Vec<u8>, file: &mut File) -> Result<Dataset, LoadError> {
	file.read_exact(&mut buffer[7..Dataset::VER3_DICT_OFFSET + 8])
		.map_err(|_| LoadError::InvalidFileSize)?;
	let resolution = u16::from_le_bytes(buffer[7..9].try_into().unwrap());
	let height_resolution = u16::from_le_bytes(buffer[9..11].try_into().unwrap());
	let metadata = TileMetadata {
		version: 3,
		resolution,
		height_resolution,
		tiling: 1,
	};

	let tile_map = buffer[Dataset::VER3_TILE_MAP_OFFSET..Dataset::VER3_DICT_OFFSET]
		.chunks_exact(8)
		.map(|x| u64::from_le_bytes(x.try_into().unwrap()))
		.collect();
	let dict_size = u64::from_le_bytes(
		buffer[Dataset::VER3_DICT_OFFSET..Dataset::VER3_DICT_OFFSET + 8]
			.try_into()
			.unwrap(),
	);
	buffer.resize(dict_size as usize, 0);

	file.read_exact(buffer).map_err(|_| LoadError::InvalidFileSize)?;
	let data_offset = Dataset::VER3_DICT_OFFSET + dict_size as usize + 8;

	Ok(Dataset {
		metadata,
		tile_map,
		dictionary: DecoderDictionary::copy(&buffer),
		data: unsafe { MmapOptions::new().offset(data_offset as _).map(&*file)? },
		data_offset,
	})
}
