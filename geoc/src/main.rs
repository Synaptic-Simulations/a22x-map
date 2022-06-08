use clap::{Parser, Subcommand};

#[cfg(feature = "generate")]
use crate::generate::Generate;
use crate::{metadata::Metadata, upgrade::Upgrade};

mod common;
#[cfg(feature = "generate")]
mod generate;
mod metadata;
#[cfg(feature = "generate")]
mod source;
mod upgrade;

#[derive(Parser)]
struct Options {
	#[clap(subcommand)]
	command: Command,
}

#[derive(Subcommand)]
enum Command {
	#[cfg(feature = "generate")]
	Generate(Generate),
	Upgrade(Upgrade),
	Metadata(Metadata),
}

fn main() {
	let opts: Options = Options::parse();
	match opts.command {
		#[cfg(feature = "generate")]
		Command::Generate(generate) => generate::generate(generate),
		Command::Upgrade(upgrade) => upgrade::upgrade(upgrade),
		Command::Metadata(metadata) => metadata::metadata(metadata),
	}
}
