use std::{
	fs::File,
	io::Read,
	path::{Path, PathBuf},
};

use bitpacking::{BitPacker, BitPacker8x};
use lz4::Decoder;

use crate::{LoadError, TileMetadata};

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

impl TileMetadata {
	pub fn load_from_directory(dir: &Path) -> Result<Self, std::io::Error> {
		let mut path = dir.to_path_buf();
		path.push("_meta");

		let mut file = File::open(path)?;
		let mut buf = Vec::new();
		file.read_to_end(&mut buf)?;

		let version = u16::from_le_bytes(buf[0..2].try_into().unwrap());
		let resolution = u16::from_le_bytes(buf[2..4].try_into().unwrap());
		let height_resolution = u16::from_le_bytes(buf[4..6].try_into().unwrap());

		Ok(Self {
			version,
			resolution,
			height_resolution,
		})
	}
}

pub struct GeoTile {
	data: Vec<u8>,
}

impl GeoTile {
	pub fn get_file_name_for_coordinates(buf: &mut PathBuf, lat: i16, lon: i16) {
		buf.push(format!(
			"{}{}{}{}.geo",
			if lat < 0 { 'S' } else { 'N' },
			lat.abs(),
			if lon < 0 { 'W' } else { 'E' },
			lon.abs()
		));
	}

	fn get_coordinates_from_file_name(path: &Path) -> (i16, i16) {
		let file_name = path.file_name().unwrap().to_str().unwrap();
		let e_or_w_location = file_name.find(['E', 'W']).unwrap();
		let mut lat: i16 = file_name[1..e_or_w_location].parse().unwrap();
		if &file_name[0..1] == "S" {
			lat = -lat;
		}
		let mut lon: i16 = file_name[e_or_w_location + 1..file_name.len() - 4].parse().unwrap();
		if &file_name[e_or_w_location..e_or_w_location + 1] == "W" {
			lon = -lon;
		}

		(lat, lon)
	}

	pub fn load(metadata: &TileMetadata, path: &Path) -> Result<Self, LoadError> {
		match metadata.version {
			1 => load_v1(path),
			2 => load_v2(path),
			_ => Err(LoadError::UnknownFormatVersion),
		}
	}

	pub fn expand(&self, metadata: &TileMetadata) -> Vec<i16> {
		let min_height = u16::from_le_bytes(self.data[0..2].try_into().unwrap());
		let bits = self.data[2];

		if bits == 0 {
			let positive_height = min_height * metadata.height_resolution;
			let height = positive_height as i16 - 500; // Lowest altitude is -414m.
			vec![height; metadata.resolution as usize * metadata.resolution as usize]
		} else {
			let mut out = vec![0; metadata.resolution as usize * metadata.resolution as usize];
			let block_size = BitPacker8x::compressed_block_size(bits);
			let packer = BitPacker8x::new();
			for (i, chunk) in out.chunks_exact_mut(BitPacker8x::BLOCK_LEN).enumerate() {
				let compressed = &self.data[3 + block_size * i..];
				packer.decompress(compressed, chunk, bits);
			}

			out.into_iter()
				.map(|height| {
					let positive_height = (height as u16 + min_height) * metadata.height_resolution;
					positive_height as i16 - 500 // Lowest altitude is -414m.
				})
				.collect()
		}
	}
}
