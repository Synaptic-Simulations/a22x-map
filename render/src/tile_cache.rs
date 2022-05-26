use std::path::PathBuf;

use geo::{GeoTile, LoadError, TileMetadata};
use wgpu::{
	util::DeviceExt,
	Device,
	Extent3d,
	Queue,
	Texture,
	TextureDescriptor,
	TextureDimension,
	TextureUsages,
	TextureView,
	TextureViewDescriptor,
};

use crate::{
	range::{Mode, Range, RANGES, RANGE_TO_DEGREES},
	LatLon,
	TextureFormat,
};

struct Metadata {
	metadata: TileMetadata,
	dir: PathBuf,
}

pub struct TileCache {
	position: LatLon,
	range: Range,
	metadata: Vec<Metadata>,
	lods: Vec<usize>,
	tile_map: Texture,
	tile_map_view: TextureView,
	atlas: Texture,
	atlas_view: TextureView,
	width: u32,
	height: u32,
}

impl TileCache {
	pub fn new(
		device: &Device, queue: &Queue, datasets: Vec<PathBuf>, position: LatLon, range: Range, mode: Mode,
	) -> Result<Self, LoadError> {
		let metadata: Result<Vec<_>, std::io::Error> = datasets
			.into_iter()
			.map(|dir| {
				Ok(Metadata {
					metadata: TileMetadata::load_from_directory(&dir)?,
					dir,
				})
			})
			.collect();
		let metadata = metadata?;
		let lods: Vec<_> = RANGE_TO_DEGREES
			.iter()
			.map(|&angle| Self::get_lod_for_range(angle, &metadata))
			.collect();

		let (width, height) = Self::get_resolution(&lods, &metadata);
		let delta_lat = LatLon {
			lat: range.vertical_degrees() / 2.0,
			lon: range.horizontal_degrees(mode) / 2.0,
		};
		let min_pos = position - delta_lat;
		let max_pos = position + delta_lat;
		let meta = &metadata[lods[range as usize]];

		let res = meta.metadata.resolution as u32;
		let slots_per_row = width / res;

		let mut curr = 0;
		let mut atlas_data = vec![0; (width * height) as usize];
		let mut tile_map_data = vec![(width, height); 360 * 180];

		for lat in min_pos.lat.floor() as i16..=max_pos.lat.floor() as i16 {
			for lon in min_pos.lon.floor() as i16..=max_pos.lon.floor() as i16 {
				let mut path = meta.dir.clone();
				GeoTile::get_file_name_for_coordinates(&mut path, lat, lon);
				let tile = match GeoTile::load(&meta.metadata, &path) {
					Ok(tile) => tile,
					Err(e) => match e {
						LoadError::Io(_) => continue,
						x => return Err(x),
					},
				};

				let row = curr / slots_per_row;
				let col = curr - row * slots_per_row;

				let offset_w = col as u32 * res;
				let offset_h = row as u32 * res;

				let data = tile.expand(&meta.metadata);
				Self::copy_tile(&mut atlas_data, &data, (offset_w, offset_h), width, res);
				curr += 1;

				tile_map_data[Self::get_index_for_lat_lon(lat, lon)] = (col as u32 * res, row as u32 * res);
			}
		}

		let atlas = device.create_texture_with_data(
			queue,
			&TextureDescriptor {
				label: Some("Heightmap Atlas"),
				size: Extent3d {
					width,
					height,
					depth_or_array_layers: 1,
				},
				mip_level_count: 1,
				sample_count: 1,
				dimension: TextureDimension::D2,
				format: TextureFormat::R16Sint,
				usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
			},
			unsafe { std::slice::from_raw_parts(atlas_data.as_ptr() as _, atlas_data.len() * 2) },
		);
		let atlas_view = atlas.create_view(&TextureViewDescriptor {
			label: Some("Heightmap Atlas View"),
			..Default::default()
		});

		let tile_map = device.create_texture_with_data(
			queue,
			&TextureDescriptor {
				label: Some("Tile Map"),
				size: Extent3d {
					width: 360,
					height: 180,
					depth_or_array_layers: 1,
				},
				mip_level_count: 1,
				sample_count: 1,
				dimension: TextureDimension::D2,
				format: TextureFormat::Rg32Uint,
				usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
			},
			unsafe { std::slice::from_raw_parts(tile_map_data.as_ptr() as _, tile_map_data.len() * 8) },
		);
		let tile_map_view = tile_map.create_view(&TextureViewDescriptor {
			label: Some("Tile Map View"),
			..Default::default()
		});

		Ok(Self {
			position,
			range,
			metadata,
			lods,
			tile_map,
			tile_map_view,
			atlas,
			atlas_view,
			width,
			height,
		})
	}

	// pub fn metadata(&self, lod: usize) -> &TileMetadata { &self.metadata[lod].metadata }

	pub fn tile_map(&self) -> &TextureView { &self.tile_map_view }

	pub fn atlas(&self) -> &TextureView { &self.atlas_view }

	pub fn tile_size_for_range(&self, range: Range) -> u32 {
		self.metadata[self.lods[range as usize]].metadata.resolution as _
	}

	pub fn atlas_resolution(&self) -> (u32, u32) { (self.width, self.height) }

	fn copy_tile(dest: &mut [i16], tile: &[i16], (offset_x, offset_y): (u32, u32), dest_row: u32, tile_row: u32) {
		let start = (offset_y * dest_row + offset_x) as usize;
		let dest = &mut dest[start..];
		for (i, row) in tile.chunks_exact(tile_row as _).enumerate() {
			let start = dest_row as usize * i;
			dest[start..start + tile_row as usize].copy_from_slice(row);
		}
	}

	fn get_resolution(lods: &[usize], metadata: &[Metadata]) -> (u32, u32) {
		let mut max_resolution = 0;
		let mut max_range = Range::Nm2;
		for (&lod, &range) in lods.iter().zip(RANGES.iter()) {
			let resolution = range.vertical_tiles_loaded() * metadata[lod].metadata.resolution as u32;
			if resolution > max_resolution {
				max_resolution = resolution;
				max_range = range;
			}
		}

		(
			max_range.horizontal_tiles_loaded(Mode::FullPage)
				* metadata[lods[max_range as usize]].metadata.resolution as u32,
			max_resolution as u32,
		)
	}

	fn get_lod_for_range(vertical_angle: f32, metadata: &[Metadata]) -> usize {
		for (lod, meta) in metadata.iter().enumerate().rev() {
			let pixels_on_screen = meta.metadata.resolution as f32 * vertical_angle;
			if pixels_on_screen >= 1040.0 {
				return lod;
			}
		}

		0
	}

	fn get_index_for_lat_lon(lat: i16, lon: i16) -> usize {
		let lat = lat + 90;
		let lon = lon + 180;
		lat as usize * 360 + lon as usize
	}
}
