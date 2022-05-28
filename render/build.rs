use hassle_rs::HassleError;
use walkdir::WalkDir;

fn main() {
	println!("cargo:rerun-if-changed=.");
	let out_dir = std::env::var("OUT_DIR").unwrap();

	let mut error = false;
	for shader in WalkDir::new("src/shaders")
		.into_iter()
		.filter_map(|e| e.ok())
		.filter(|e| e.path().is_file())
	{
		let input = shader.path();
		let file_name = shader.path().file_name().unwrap().to_str().unwrap();

		let ty = if file_name.contains("VS") {
			Some("vs_6_0")
		} else if file_name.contains("PS") {
			Some("ps_6_0")
		} else {
			None
		};

		if let Some(ty) = ty {
			let bytecode = match hassle_rs::compile_hlsl(
				input.to_str().unwrap(),
				&std::fs::read_to_string(input).unwrap(),
				"Main",
				ty,
				&vec!["-spirv", "-fspv-debug=line"],
				&[],
			) {
				Ok(bytecode) => bytecode,
				Err(HassleError::CompileError(e)) => {
					eprintln!("{}", e);
					error = true;
					continue;
				},
				Err(x) => panic!("{:?}", x),
			};

			let out_file = format!("{}/{}", out_dir, file_name);
			std::fs::write(&out_file, bytecode).unwrap();
			println!("cargo:rustc-env={}={}", file_name, out_file);
		}
	}

	if error {
		panic!("Compilation failed");
	}
}
