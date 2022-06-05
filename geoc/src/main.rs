use clap::{Parser, Subcommand};

use crate::{generate::Generate, metadata::Metadata, upgrade::Upgrade};

// mod downsample;
mod generate;
mod metadata;
mod source;
mod upgrade;

#[derive(Parser)]
struct Options {
	#[clap(subcommand)]
	command: Command,
}

#[derive(Subcommand)]
enum Command {
	Generate(Generate),
	Upgrade(Upgrade),
	// Downsample(Downsample),
	Metadata(Metadata),
}

fn main() {
	let opts: Options = Options::parse();
	match opts.command {
		Command::Generate(generate) => generate::generate(generate),
		Command::Upgrade(upgrade) => upgrade::upgrade(upgrade),
		// Command::Downsample(downsample) => downsample::downsample(downsample),
		Command::Metadata(metadata) => metadata::metadata(metadata),
	}
}
