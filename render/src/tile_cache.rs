use std::{num::NonZeroU32, path::PathBuf};

use geo::{Dataset, LoadError};
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
	TextureFormat,
	TextureUsages,
	TextureView,
	TextureViewDescriptor,
};

use crate::range::{Range, RANGES, RANGE_TO_DEGREES, radians_per_pixel};

pub enum UploadStatus {
	Uploads,
	NoUploads,
	Resized,
	AtlasFull,
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

	pub fn populate_tiles(&mut self, device: &Device, queue: &Queue, height: u32, vertical_angle: f32) -> UploadStatus {
		tracy::zone!("Tile Population");

		let radians_per_pixel = radians_per_pixel(height as _, vertical_angle);

		if self.atlas.needs_clear() {
			self.clear(range);
		}
		let meta = self.atlas.lods[range as usize];

		let mut ret = UploadStatus::NoUploads;
		{
			let _ = self.tile_status.slice(..).map_async(MapMode::Read);

			{
				tracy::zone!("GPU Readback Sync");
				device.poll(Maintain::Wait);
			}

			let buf = self.tile_status.slice(..).get_mapped_range();
			let used = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u32, buf.len() / 4) };

			'outer: for lon in 0..360 {
				for lat in 0..180 {
					let index = (lat * 360 + lon) as usize;
					let offset = &mut self.tiles[index];
					if used[index] == 0 {
						if *offset != self.atlas.unloaded() && *offset != self.atlas.not_found() {
							self.atlas.return_tile(*offset);
							*offset = self.atlas.unloaded();
						}
						continue;
					} else if *offset != self.atlas.unloaded() {
						continue;
					}

					ret = UploadStatus::Uploads;
					let lon = lon as i16 - 180;
					let lat = lat as i16 - 90;
					let dataset = &self.atlas.datasets[meta];
					let tile = {
						tracy::zone!("Load Tile");

						if let Some(data) = dataset.get_tile(lat, lon) {
							match data {
								Ok(x) => x,
								Err(e) => {
									log::error!("Error loading tile: {:?}", e);
									continue;
								},
							}
						} else {
							*offset = self.atlas.not_found();
							continue;
						}
					};

					self.tiles[index] = if let Some(offset) = self.atlas.upload_tile(queue, &tile) {
						offset
					} else if self.atlas.collect_tiles(used, &mut self.tiles, index) {
						self.atlas
							.upload_tile(queue, &tile)
							.expect("Tile GC returned None when it had to be Some")
					} else {
						if self.atlas.recreate_atlas(device) {
							self.tiles.fill(self.atlas.unloaded());
							ret = UploadStatus::Resized;
						} else {
							ret = UploadStatus::AtlasFull;
						}
						break 'outer;
					};
				}
			}
		}

		self.tile_status.unmap();

		{
			if let UploadStatus::Uploads | UploadStatus::Resized = ret {
				tracy::zone!("Tile Map Upload");

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
			}
		}

		ret
	}

	pub fn clear(&mut self) {
		for offset in self.tiles.iter_mut() {
			*offset = self.atlas.unloaded();
		}
		self.atlas.clear(range);
	}

	pub fn tile_map(&self) -> &TextureView { &self.tile_map_view }

	pub fn tile_status(&self) -> &Buffer { &self.tile_status }

	pub fn atlas(&self) -> &TextureView { &self.atlas.view }

	pub fn hillshade(&self) -> &TextureView { &self.atlas.hillshade_view }

	pub fn tile_size_for_angle(&self, vertical_angle: f32) -> u32 {

	}
}

struct Atlas {
	datasets: Vec<Dataset>,
	lod_densities: Vec<f32>,
	atlas: Texture,
	view: TextureView,
	hillshade: Texture,
	hillshade_view: TextureView,
	width: u32,
	height: u32,
	curr_dataset: usize,
	curr_offset: TileOffset,
	collected_tiles: Vec<TileOffset>,
}

impl Atlas {
	fn new(device: &Device, datasets: Vec<PathBuf>) -> Result<Self, LoadError> {
		let datasets: Result<Vec<_>, LoadError> = datasets.into_iter().map(|dir| Dataset::load(&dir)).collect();
		let datasets = datasets?;

		let lod_densities = datasets.iter().map(|x| radians_per_pixel(x.metadata().resolution as _, 1.0f32.to_radians())).collect();

		let (width, height) = (4096, 4096);
		let limits = device.limits();
		let width = width.min(limits.max_texture_dimension_2d);
		let height = height.min(limits.max_texture_dimension_2d);
		let (atlas, view, hillshade, hillshade_view) = Self::make_atlas(device, width, height);

		Ok(Self {
			datasets,
			lod_densities,
			atlas,
			view,
			hillshade,
			hillshade_view,
			width,
			height,
			curr_dataset: datasets.len(),
			curr_offset: TileOffset::default(),
			collected_tiles: Vec::new(),
		})
	}

	fn get_dataset_for_angle(&self, radians_per_pixel: f32) -> usize {
		let mut index = 0;
		for (i, &density) in self.lod_densities.iter().enumerate().rev() {
			if radians_per_pixel <= density {
				index = i;
				break;
			}
		}

		index
	}

	fn needs_clear(&self, radians_per_pixel: f32) -> bool {
		self.get_dataset_for_angle(radians_per_pixel) != self.curr_dataset
	}

	fn clear(&mut self) {
		self.curr_offset = TileOffset::default();
		self.collected_tiles.clear();
	}

	fn return_tile(&mut self, tile: TileOffset) { self.collected_tiles.push(tile); }

	fn upload_tile(&mut self, queue: &Queue, tile: &[u16]) -> Option<TileOffset> {
		tracy::zone!("Tile Upload");

		let ret = if let Some(tile) = self.collected_tiles.pop() {
			tile
		} else {
			let ret = self.curr_offset;
			if ret.y + (self.curr_tile_res) >= self.height {
				return None;
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
				bytes_per_row: Some(NonZeroU32::new(2 * self.curr_tile_res).unwrap()),
				rows_per_image: Some(NonZeroU32::new(self.curr_tile_res).unwrap()),
			},
			Extent3d {
				width: self.curr_tile_res,
				height: self.curr_tile_res,
				depth_or_array_layers: 1,
			},
		);

		self.curr_offset.x += self.curr_tile_res;
		if self.curr_offset.x + self.curr_tile_res >= self.width {
			self.curr_offset.x = 0;
			self.curr_offset.y += self.curr_tile_res;
		}

		Some(ret)
	}

	fn collect_tiles(&mut self, used: &[u32], tiles: &mut [TileOffset], start: usize) -> bool {
		tracy::zone!("Tile GC");

		let mut needed = 1;
		let mut collected = 0;
		for (&used, offset) in used[start + 1..].iter().zip(tiles[start + 1..].iter_mut()) {
			if used == 1 && *offset == self.unloaded() {
				needed += 1;
			} else {
				if *offset != self.unloaded() && *offset != self.not_found() {
					self.collected_tiles.push(*offset);
					*offset = self.unloaded();
					collected += 1;
				}
			}
		}

		collected >= needed
	}

	fn recreate_atlas(&mut self, device: &Device) -> bool {
		let limits = device.limits();
		if self.width == limits.max_texture_dimension_2d && self.height == limits.max_texture_dimension_2d {
			log::error!("Atlas is too large to fit in device limits");
			return false;
		}

		let width = (self.width * 2).min(limits.max_texture_dimension_2d);
		let height = (self.height * 2).min(limits.max_texture_dimension_2d);
		let (atlas, view, hillshade, hillshade_view) = Self::make_atlas(device, width, height);

		self.atlas = atlas;
		self.view = view;
		self.hillshade = hillshade;
		self.hillshade_view = hillshade_view;
		self.width = width;
		self.height = height;
		self.curr_tile_res = 0;

		true
	}

	fn make_atlas(device: &Device, width: u32, height: u32) -> (Texture, TextureView, Texture, TextureView) {
		let descriptor = TextureDescriptor {
			label: Some("Heightmap Atlas"),
			size: Extent3d {
				width,
				height,
				depth_or_array_layers: 1,
			},
			mip_level_count: 1,
			sample_count: 1,
			dimension: TextureDimension::D2,
			format: TextureFormat::R16Uint,
			usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
		};

		let atlas = device.create_texture(&descriptor);
		let view = atlas.create_view(&TextureViewDescriptor {
			label: Some("Heightmap Atlas View"),
			..Default::default()
		});

		let hillshade = device.create_texture(&TextureDescriptor {
			label: Some("Hillshade"),
			format: TextureFormat::R8Unorm,
			usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
			..descriptor
		});
		let hillshade_view = hillshade.create_view(&TextureViewDescriptor {
			label: Some("Hillshade View"),
			..Default::default()
		});

		(atlas, view, hillshade, hillshade_view)
	}

	fn unloaded(&self) -> TileOffset { TileOffset { x: 0, y: self.height } }

	fn not_found(&self) -> TileOffset { TileOffset { x: self.width, y: 0 } }
}
