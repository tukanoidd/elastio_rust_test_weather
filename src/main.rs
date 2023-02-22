mod config;
mod data;
mod providers;
mod ui;

use clap::{Parser, Subcommand};
use color_eyre::eyre;

use crate::providers::Provider;
use crate::ui::draw_data;

pub(crate) mod built_info {
    // The file has been placed there by the build script.
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Configure {
        provider: String,
    },
    Get {
        address: String,
        #[clap(default_value = "now")]
        date: String,
    },
}

fn main() -> eyre::Result<()> {
    // Set up colorized error messages
    color_eyre::install()?;

    // Parse command line arguments
    let Args { command } = Args::parse();

    // Get config
    let mut config = config::Config::new()?;

    match command {
        Command::Configure { provider } => {
            let provider = Provider::from_str(provider)?;

            config.provider = provider;

            config.save()?;
        }
        Command::Get { address, date } => {
            let data = config.provider.get(address, date)?;
            draw_data(data)?;
        }
    }

    Ok(())
}
