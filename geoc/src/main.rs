use geo::tiff::GeoTiff;

fn main() {
	GeoTiff::parse(&std::fs::read("D:\\Code\\Topography\\NASADEM_be\\NASADEM_HGT_n00w051.tif").unwrap()).unwrap();
}
