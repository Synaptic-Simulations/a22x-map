use std::{
	error::Error,
	fs::DirEntry,
	path::Path,
	sync::atomic::{AtomicUsize, Ordering},
};

use rayon::prelude::*;

pub fn for_each_file(path: &Path, f: impl (Fn(DirEntry) -> Result<(), Box<dyn Error>>) + Send + Sync) {
	let dir = match std::fs::read_dir(path) {
		Ok(x) => x,
		Err(e) => {
			eprintln!("{}", e);
			std::process::exit(1);
		},
	};

	let num = std::fs::read_dir(path)
		.unwrap()
		.filter(|x| {
			if let Ok(x) = x {
				if let Some(ex) = x.path().extension() {
					let ex = ex.to_string_lossy();
					ex == "geo" || ex == "tiff" || ex == "tif"
				} else {
					false
				}
			} else {
				false
			}
		})
		.count();
	let done = AtomicUsize::new(0);

	dir.par_bridge().for_each(|entry| {
		let entry = match entry {
			Ok(x) => x,
			Err(e) => {
				eprintln!("{}", e);
				return;
			},
		};

		let path = entry.path();
		if let Some(ex) = path.extension() {
			let ex = ex.to_string_lossy();
			if ex == "geo" || ex == "tiff" || ex == "tif" {
				match f(entry) {
					Ok(_) => {},
					Err(e) => {
						eprintln!("error in file {}: {}", path.display(), e);
						return;
					},
				}
			}

			done.fetch_add(1, Ordering::SeqCst);
			println!("{}/{}", done.load(Ordering::SeqCst), num);
		}
	})
}
