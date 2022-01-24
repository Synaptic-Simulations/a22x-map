use crate::tiff::GeoTiff;

pub mod tiff;

fn main() {
	let tiff =
		GeoTiff::parse(&std::fs::read("D:\\Code\\Topography\\NASADEM_be\\NASADEM_HGT_n01e111.tif").unwrap()).unwrap();

	let mut min = i16::MAX;
	let mut max = i16::MIN;
	let mut sum: i64 = 0;
	for x in tiff.data.iter() {
		min = min.min(*x);
		max = max.max(*x);
		sum += *x as i64;
	}

	println!("min: {}", min);
	println!("max: {}", max);
	println!("mean: {}", sum as f32 / tiff.data.len() as f32);

	let data = geo::compress(tiff.downsample().into_iter());
	let data = geo::decompress(&data);

	let mut min = i16::MAX;
	let mut max = i16::MIN;
	let mut sum: i64 = 0;
	for x in data.iter() {
		min = min.min(*x);
		max = max.max(*x);
		sum += *x as i64;
	}

	println!("min: {}", min);
	println!("max: {}", max);
	println!("mean: {}", sum as f32 / data.len() as f32);
}
