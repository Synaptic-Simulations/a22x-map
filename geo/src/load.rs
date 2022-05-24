use std::{fs::File, io::Read, path::Path};

use lz4::Decoder;

use crate::{GeoTile, LoadError, TileMetadata};

pub fn load_v1(metadata: &TileMetadata, path: &Path) -> Result<GeoTile, LoadError> {
	let mut file = File::open(path)?;
	let mut buf = [0; 2];
	file.read_exact(&mut buf)?;
	let min_height = u16::from_le_bytes(buf);
	file.read_exact(&mut buf[0..1])?;
	let bits = u8::from_le_bytes([buf[0]]);

	let res = metadata.resolution as usize * metadata.resolution as usize;
	let total_bits = res * bits as usize;
	let bytes = (total_bits + 7) / 8;
	let mut data = Vec::with_capacity(bytes);
	file.read_to_end(&mut data)?;

	Ok(GeoTile { min_height, bits, data })
}

pub fn load_v2(metadata: &TileMetadata, path: &Path) -> Result<GeoTile, LoadError> {
	let mut decoder = Decoder::new(File::open(path)?)?;
	let mut buf = [0; 2];
	decoder.read_exact(&mut buf)?;
	let min_height = u16::from_le_bytes(buf);
	decoder.read_exact(&mut buf[0..1])?;
	let bits = u8::from_le_bytes([buf[0]]);

	let res = metadata.resolution as usize * metadata.resolution as usize;
	let total_bits = res * bits as usize;
	let bytes = (total_bits + 7) / 8;
	let mut data = Vec::with_capacity(bytes);
	decoder.read_to_end(&mut data)?;

	Ok(GeoTile { min_height, bits, data })
}
