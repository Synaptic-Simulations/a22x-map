use std::{collections::VecDeque, path::PathBuf};

use geo::{GeoTile, LoadError, TileMetadata};
use wgpu::{
	util::{BufferInitDescriptor, DeviceExt},
	Buffer,
	BufferDescriptor,
	BufferUsages,
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
	range::{lod_for_range, tiles_for_range, Range},
	LatLon,
	TextureFormat,
};

struct Metadata {
	metadata: TileMetadata,
	dir: PathBuf,
}

pub struct TileCache {
	last_load_position: LatLon,
	range: Range,
	metadata: [Metadata; 3],
	tile_indices: Vec<u32>,
	tile_map: Texture,
	tile_map_view: TextureView,
	index_manager: BufferIndexManager,
	tiles: Vec<Buffer>,
}

impl TileCache {
	pub fn new(
		device: &Device, queue: &Queue, position: LatLon, range: Range, path: PathBuf,
	) -> Result<Self, LoadError> {
		let path_1024 = path.join("1024");
		let path_512 = path.join("512");
		let path_128 = path.join("128");
		let metadata = [
			Metadata {
				metadata: TileMetadata::load_from_directory(&path_1024)?,
				dir: path_1024,
			},
			Metadata {
				metadata: TileMetadata::load_from_directory(&path_512)?,
				dir: path_512,
			},
			Metadata {
				metadata: TileMetadata::load_from_directory(&path_128)?,
				dir: path_128,
			},
		];

		let lat = position.lat.floor() as i16;
		let lon = position.lon.floor() as i16;
		let tiles = tiles_for_range(range);
		let min_lat = lat - tiles;
		let max_lat = lat + tiles;
		let min_lon = lon - tiles;
		let max_lon = lon + tiles;

		let dummy_buffer = device.create_buffer(&BufferDescriptor {
			label: Some("Dummy Buffer"),
			size: 32,
			usage: BufferUsages::STORAGE,
			mapped_at_creation: false,
		});

		let curr_metadata = &metadata[lod_for_range(range)];

		let mut tile_indices = vec![0; 360 * 180];
		let mut tiles = vec![dummy_buffer];
		for lat in min_lat..=max_lat {
			for lon in min_lon..=max_lon {
				let mut path = curr_metadata.dir.clone();
				GeoTile::get_file_name_for_coordinates(&mut path, lat, lon);
				let tile = match GeoTile::load(&curr_metadata.metadata, &path) {
					Ok(tile) => tile,
					Err(_) => {
						log::error!("Failed to load tile: {}", path.display());
						continue;
					},
				};

				let chunk = tile.chunk();
				let buffer = device.create_buffer_init(&BufferInitDescriptor {
					label: Some(&format!("Tile {} {}", lat, lon)),
					contents: chunk,
					usage: BufferUsages::STORAGE,
				});

				let tile_index = Self::get_index_for_lat_lon(lat, lon);
				tile_indices[tile_index] = tiles.len() as _;
				tiles.push(buffer);
			}
		}

		let tile_map = device.create_texture_with_data(
			&queue,
			&TextureDescriptor {
				label: Some("Tile Map"),
				size: Extent3d {
					width: 180,
					height: 360,
					depth_or_array_layers: 1,
				},
				mip_level_count: 1,
				sample_count: 1,
				dimension: TextureDimension::D2,
				format: TextureFormat::R32Uint,
				usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
			},
			unsafe { std::slice::from_raw_parts(tile_indices.as_ptr() as _, tile_indices.len() * 4) },
		);

		let tile_map_view = tile_map.create_view(&TextureViewDescriptor {
			label: Some("Tile Map View"),
			..Default::default()
		});

		Ok(Self {
			last_load_position: position,
			range,
			metadata,
			tile_indices,
			tile_map,
			tile_map_view,
			index_manager: BufferIndexManager::new(tiles.len() as _),
			tiles,
		})
	}

	pub fn get_tile_map(&self) -> &TextureView { &self.tile_map_view }

	pub fn get_tile_buffers(&self) -> &[Buffer] { &self.tiles }

	fn get_index_for_lat_lon(lat: i16, lon: i16) -> usize {
		let lat = lat + 90;
		let lon = lon + 180;
		lon as usize * 180 + lat as usize
	}
}

pub struct BufferIndexManager {
	returned: VecDeque<u32>,
	unused: std::ops::Range<u32>,
}

impl BufferIndexManager {
	fn new(used_indices: u32) -> Self {
		Self {
			returned: VecDeque::new(),
			unused: used_indices..360 * 180,
		}
	}
}
