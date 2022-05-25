use std::{fs::File, io::Read, path::Path};

use lz4::Decoder;

use crate::{GeoTile, LoadError};

pub fn load_v1(path: &Path) -> Result<GeoTile, LoadError> {
	let mut file = File::open(path)?;

	let mut data = Vec::new();
	file.read_to_end(&mut data)?;

	Ok(GeoTile { data })
}

pub fn load_v2(path: &Path) -> Result<GeoTile, LoadError> {
	let mut decoder = Decoder::new(File::open(path)?)?;

	let mut data = Vec::new();
	decoder.read_to_end(&mut data)?;

	Ok(GeoTile { data })
}
