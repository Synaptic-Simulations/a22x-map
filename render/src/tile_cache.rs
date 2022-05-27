use std::{num::NonZeroU32, path::PathBuf};

use geo::{GeoTile, LoadError, TileMetadata};
use wgpu::{
	Buffer,
	BufferDescriptor,
	BufferUsages,
	Device,
	Extent3d,
	ImageCopyTexture,
	ImageDataLayout,
	Maintain,
	MapMode,
	Origin3d,
	Queue,
	Texture,
	TextureAspect,
	TextureDescriptor,
	TextureDimension,
	TextureUsages,
	TextureView,
	TextureViewDescriptor,
};

use crate::{
	range::{Mode, Range, RANGES, RANGE_TO_DEGREES},
	TextureFormat,
};

struct Metadata {
	metadata: TileMetadata,
	dir: PathBuf,
}

#[repr(C)]
#[derive(Copy, Clone, Default, PartialEq, Eq)]
struct TileOffset {
	x: u32,
	y: u32,
}

pub struct TileCache {
	tile_map: Texture,
	tile_map_view: TextureView,
	tile_status: Buffer,
	atlas: Atlas,
	tiles: Vec<TileOffset>,
}

impl TileCache {
	pub fn new(device: &Device, datasets: Vec<PathBuf>) -> Result<Self, LoadError> {
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
			format: TextureFormat::Rg32Uint,
			usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
		});
		let tile_map_view = tile_map.create_view(&TextureViewDescriptor {
			label: Some("Tile Map View"),
			..Default::default()
		});

		let tile_status = device.create_buffer(&BufferDescriptor {
			label: Some("Tile Status"),
			size: 360 * 180 * 4,
			usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ | BufferUsages::STORAGE,
			mapped_at_creation: false,
		});

		let atlas = Atlas::new(device, datasets)?;

		Ok(Self {
			tile_map,
			tile_map_view,
			tile_status,
			tiles: vec![atlas.unloaded(); 360 * 180],
			atlas,
		})
	}

	pub fn populate_tiles(&mut self, device: &Device, queue: &Queue, range: Range) -> bool {
		if self.atlas.needs_clear(range) {
			self.clear();
		}
		let meta = self.atlas.lods[range as usize];

		{
			let _ = self.tile_status.slice(..).map_async(MapMode::Read);
			device.poll(Maintain::Wait);
			let buf = self.tile_status.slice(..).get_mapped_range();
			let used = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u32, buf.len() / 4) };
			for lon in 0..360 {
				for lat in 0..180 {
					let index = (lat * 360 + lon) as usize;
					let offset = &mut self.tiles[index];
					if *offset != self.atlas.unloaded() || used[index] == 0 {
						continue;
					}

					let lon = lon as i16 - 180;
					let lat = lat as i16 - 90;

					let metadata = &self.atlas.metadata[meta];
					let mut path = metadata.dir.clone();
					GeoTile::get_file_name_for_coordinates(&mut path, lat, lon);
					let tile = match GeoTile::load(&metadata.metadata, &path) {
						Ok(x) => x,
						Err(_) => {
							*offset = self.atlas.not_found();
							continue;
						},
					}
					.expand(&metadata.metadata);

					let offset =
						if let Some(offset) = self.atlas.upload_tile(device, queue, &tile, &used, &mut self.tiles) {
							offset
						} else {
							std::mem::drop(buf);
							self.tile_status.unmap();
							return true;
						};
					self.tiles[index] = offset;
				}
			}
		}

		self.tile_status.unmap();

		queue.write_texture(
			self.tile_map.as_image_copy(),
			unsafe {
				std::slice::from_raw_parts(
					self.tiles.as_ptr() as _,
					self.tiles.len() * std::mem::size_of::<TileOffset>(),
				)
			},
			ImageDataLayout {
				offset: 0,
				bytes_per_row: Some(NonZeroU32::new(std::mem::size_of::<TileOffset>() as u32 * 360).unwrap()),
				rows_per_image: Some(NonZeroU32::new(180).unwrap()),
			},
			Extent3d {
				width: 360,
				height: 180,
				depth_or_array_layers: 1,
			},
		);

		false
	}

	pub fn clear(&mut self) {
		for offset in self.tiles.iter_mut() {
			*offset = self.atlas.unloaded();
		}
		self.atlas.clear();
	}

	pub fn tile_map(&self) -> &TextureView { &self.tile_map_view }

	pub fn tile_status(&self) -> &Buffer { &self.tile_status }

	pub fn atlas(&self) -> &TextureView { &self.atlas.view }

	pub fn tile_size_for_range(&self, range: Range) -> u32 {
		self.atlas.metadata[self.atlas.lods[range as usize]].metadata.resolution as _
	}
}

struct Atlas {
	metadata: Vec<Metadata>,
	lods: Vec<usize>,
	atlas: Texture,
	view: TextureView,
	width: u32,
	height: u32,
	curr_tile_res: u16,
	curr_offset: TileOffset,
	collected_tiles: Vec<TileOffset>,
	tried_gc: bool,
}

impl Atlas {
	fn new(device: &Device, datasets: Vec<PathBuf>) -> Result<Self, LoadError> {
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
		let (atlas, view) = Self::make_atlas(device, width, height);

		Ok(Self {
			metadata,
			lods,
			atlas,
			view,
			width,
			height,
			curr_tile_res: 0,
			curr_offset: TileOffset::default(),
			collected_tiles: Vec::new(),
			tried_gc: false,
		})
	}

	fn needs_clear(&mut self, range: Range) -> bool {
		self.tried_gc = false;
		let res = self.metadata[self.lods[range as usize]].metadata.resolution;
		let ret = res != self.curr_tile_res;
		self.curr_tile_res = res;
		ret
	}

	fn clear(&mut self) {
		self.collected_tiles.clear();
		self.curr_offset = TileOffset::default();
	}

	fn upload_tile(
		&mut self, device: &Device, queue: &Queue, tile: &[i16], tiles_used: &[u32], tile_offsets: &mut [TileOffset],
	) -> Option<TileOffset> {
		let ret = if let Some(tile) = self.collected_tiles.pop() {
			tile
		} else {
			let ret = self.curr_offset;
			if ret.y + (self.curr_tile_res as u32) > self.height {
				if self.tried_gc {
					self.recreate_atlas(device);
					return None;
				}
				self.tried_gc = true;
				if !self.gc_tiles(tiles_used, tile_offsets) {
					self.recreate_atlas(device);
					return None;
				} else {
					self.collected_tiles.pop().unwrap()
				}
			} else {
				ret
			}
		};

		queue.write_texture(
			ImageCopyTexture {
				texture: &self.atlas,
				mip_level: 0,
				origin: Origin3d {
					x: ret.x as _,
					y: ret.y as _,
					z: 0,
				},
				aspect: TextureAspect::All,
			},
			unsafe { std::slice::from_raw_parts(tile.as_ptr() as _, tile.len() * 2) },
			ImageDataLayout {
				offset: 0,
				bytes_per_row: Some(NonZeroU32::new(2 * self.curr_tile_res as u32).unwrap()),
				rows_per_image: Some(NonZeroU32::new(self.curr_tile_res as u32).unwrap()),
			},
			Extent3d {
				width: self.curr_tile_res as _,
				height: self.curr_tile_res as _,
				depth_or_array_layers: 1,
			},
		);

		self.curr_offset.x += self.curr_tile_res as u32;
		if self.curr_offset.x + self.curr_tile_res as u32 >= self.width {
			self.curr_offset.x = 0;
			self.curr_offset.y += self.curr_tile_res as u32;
		}

		Some(ret)
	}

	fn gc_tiles(&mut self, tiles_used: &[u32], tile_offsets: &mut [TileOffset]) -> bool {
		let mut reclaimed = false;
		for (used, offset) in tiles_used.iter().zip(tile_offsets.iter_mut()) {
			if *used == 0 && *offset != self.unloaded() && *offset != self.not_found() {
				self.collected_tiles.push(*offset);
				*offset = self.unloaded();
				reclaimed = true;
			}
		}
		reclaimed
	}

	fn recreate_atlas(&mut self, device: &Device) {
		let limits = device.limits();
		if self.width == limits.max_texture_dimension_2d && self.height == limits.max_texture_dimension_2d {
			panic!("Atlas is already the maximum size");
		}

		let width = (self.width * 2).min(limits.max_texture_dimension_2d);
		let height = (self.height * 2).min(limits.max_texture_dimension_2d);
		let (atlas, view) = Self::make_atlas(device, width, height);

		self.atlas = atlas;
		self.view = view;
		self.width = width;
		self.height = height;
		self.curr_tile_res = 0;
	}

	fn make_atlas(device: &Device, width: u32, height: u32) -> (Texture, TextureView) {
		let atlas = device.create_texture(&TextureDescriptor {
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
		});
		let view = atlas.create_view(&TextureViewDescriptor {
			label: Some("Heightmap Atlas View"),
			..Default::default()
		});

		(atlas, view)
	}

	fn unloaded(&self) -> TileOffset { TileOffset { x: 0, y: self.height } }

	fn not_found(&self) -> TileOffset { TileOffset { x: self.width, y: 0 } }

	fn get_lod_for_range(vertical_angle: f32, metadata: &[Metadata]) -> usize {
		for (lod, meta) in metadata.iter().enumerate().rev() {
			let pixels_on_screen = meta.metadata.resolution as f32 * vertical_angle;
			if pixels_on_screen >= 1100.0 {
				return lod;
			}
		}

		0
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
