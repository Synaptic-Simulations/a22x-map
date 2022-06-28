use clap::{Parser, Subcommand};

#[cfg(feature = "generate")]
use crate::generate::Generate;
use crate::{info::Info, upgrade::Upgrade};

mod common;
// mod edit;
#[cfg(feature = "generate")]
mod generate;
mod info;
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
	Info(Info),
	// Edit(Edit),
}

fn main() {
	let opts: Options = Options::parse();
	match opts.command {
		#[cfg(feature = "generate")]
		Command::Generate(generate) => generate::generate(generate),
		Command::Upgrade(upgrade) => upgrade::upgrade(upgrade),
		Command::Info(info) => info::info(info),
		// Command::Edit(edit) => edit::edit(edit),
	}
}
