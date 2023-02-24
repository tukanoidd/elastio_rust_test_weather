mod config;
mod data;
mod providers;
mod ui;

use clap::{Parser, Subcommand};
use color_eyre::eyre;

use crate::{providers::Provider, ui::draw_data};

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
            // Check if the input provider is valid
            let provider = Provider::from_str(provider)?;

            // If yes, set the provider in the config
            config.provider = provider;

            // And save the config
            config.save()?;
        }
        Command::Get { address, date } => {
            // Get the weather data
            let data = config.provider.get(address, date)?;

            // Draw the weather data
            draw_data(data)?;
        }
    }

    Ok(())
}
