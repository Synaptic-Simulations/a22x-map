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
	tile_atlas: Atlas,
}

impl TileCache {
	pub fn new(
		device: &Device, queue: &Queue, position: LatLon, range: Range, datasets: Vec<PathBuf>,
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

		let tile_map = device.create_texture(&TextureDescriptor {
			label: Some("Tile Map"),
			size: Extent3d {
				width: 360,
				height: 180,
				depth_or_array_layers: 1,
			},
			mip_level_count: 1,
			sample_count: 1,
			dimension: TextureDimension::D2,
			format: TextureFormat::R32Uint,
			usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
		});
		let tile_map_view = tile_map.create_view(&TextureViewDescriptor {
			label: Some("Tile Map View"),
			..Default::default()
		});

		let tile_atlas = Atlas::new(device, &lods, &metadata);

		Ok(Self {
			position,
			range,
			metadata,
			lods,
			tile_map,
			tile_map_view,
			tile_atlas,
		})
	}

	pub fn metadata(&self, lod: usize) -> &TileMetadata { &self.metadata[lod].metadata }

	pub fn tile_map(&self) -> &TextureView { &self.tile_map_view }

	pub fn atlas(&self) -> &TextureView { &self.tile_atlas.view }

	pub fn tile_size_for_range(&self, range: Range) -> u32 {
		self.metadata[self.lods[range as usize]].metadata.resolution as _
	}

	pub fn atlas_resolution(&self) -> (u32, u32) { (self.tile_atlas.width, self.tile_atlas.height) }

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

pub struct Atlas {
	texture: Texture,
	view: TextureView,
	width: u32,
	height: u32,
}

impl Atlas {
	fn new(device: &Device, lods: &[usize], metadata: &[Metadata]) -> Self {
		let (width, height) = Self::get_resolution(lods, metadata);
		let texture = device.create_texture(&TextureDescriptor {
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
			usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
		});
		let view = texture.create_view(&TextureViewDescriptor {
			label: Some("Heightmap Atlas View"),
			..Default::default()
		});

		Self {
			texture,
			view,
			width,
			height,
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
}
