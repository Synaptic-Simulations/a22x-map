use clap::{Parser, Subcommand};

use crate::{generate::Generate, metadata::Metadata, upgrade::Upgrade};

mod common;
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
	Metadata(Metadata),
}

fn main() {
	let opts: Options = Options::parse();
	match opts.command {
		Command::Generate(generate) => generate::generate(generate),
		Command::Upgrade(upgrade) => upgrade::upgrade(upgrade),
		Command::Metadata(metadata) => metadata::metadata(metadata),
	}
}
