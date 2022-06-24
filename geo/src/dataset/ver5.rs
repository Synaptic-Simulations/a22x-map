use std::{fs::File, io::Read};

use libwebp_sys::WebPDecodeRGBAInto;
use memmap2::MmapOptions;

use crate::{map_lat_lon_to_index, Dataset, LoadError, TileMetadata};

pub fn load(buffer: &mut Vec<u8>, file: &mut File) -> Result<Dataset, LoadError> {
	file.read_exact(&mut buffer[7..Dataset::VER5_TILE_MAP_OFFSET + 360 * 180 * 8])
		.map_err(|_| LoadError::InvalidFileSize)?;
	let resolution = u16::from_le_bytes(buffer[7..9].try_into().unwrap());
	let height_resolution = u16::from_le_bytes(buffer[9..11].try_into().unwrap());
	let metadata = TileMetadata {
		version: 5,
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
	let frame_size = u32::from_le_bytes(frame[4..8].try_into().unwrap()) + 8;
	let frame = &frame[..frame_size as usize];

	let res = this.metadata.resolution as usize;
	let mut decompressed = Vec::with_capacity(res * res * 2);
	decompressed.resize(decompressed.capacity(), 0);

	unsafe {
		tracy::zone!("Decompress");

		if WebPDecodeRGBAInto(
			frame.as_ptr(),
			frame.len(),
			decompressed.as_mut_ptr(),
			decompressed.len(),
			this.metadata.resolution as i32 * 2,
		)
		.is_null()
		{
			return Some(Err(std::io::Error::new(
				std::io::ErrorKind::Other,
				"WebPDecodeRGBAInto failed",
			)));
		}
	};

	let mapped: Vec<_> = decompressed
		.chunks_exact(2)
		.map(|value| {
			let positive_height = u16::from_le_bytes(value.try_into().unwrap()) * this.metadata.height_resolution;
			positive_height as i16 - 500
		})
		.collect();

	let output = if this.metadata.delta_compressed {
		let mut output = Vec::with_capacity(res * res * 2);
		output.resize(output.capacity(), 0);

		output[0] = mapped[0];
		for i in 1..output.len() {
			output[i] = output[i - 1] + mapped[i];
		}

		output
	} else {
		mapped
	};

	Some(Ok((output, frame_size as usize)))
}
