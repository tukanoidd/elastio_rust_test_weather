mod config;
mod data;
mod providers;
mod ui;

use clap::builder::NonEmptyStringValueParser;
use clap::{arg, command};
use color_eyre::eyre;

use crate::{providers::Provider, ui::draw_data};

pub(crate) mod built_info {
    // The file has been placed there by the build script.
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

fn main() -> eyre::Result<()> {
    // Set up colorized error messages
    color_eyre::install()?;

    // Parse command line arguments
    let matches = command!()
        .subcommand(
            clap::Command::new("configure")
                .before_help("Configure the weather cli (only setting a provider is supported for now)")
                .arg(
                    arg!(<provider>)
                        .required(true)
                        .help("Weather API Provider")
                        .value_parser(Provider::AVAILABLE_PROVIDERS)
                )
        )
        .subcommand(
            clap::Command::new("get")
                .arg(
                    arg!(<address>)
                        .required(true)
                        .allow_hyphen_values(true)
                        .value_parser(NonEmptyStringValueParser::new())
                        .help("Address you want to get weather information from (\"lat, lon\" format is supported)")
                )
                .arg(
                    arg!([date])
                        .help("Date for which you want to get weather information (Check README for more info)")
                        .value_parser(NonEmptyStringValueParser::new())
                        .default_value("now")
                )
        ).get_matches();

    // Get config
    let mut config = config::Config::new()?;

    match matches.subcommand() {
        Some(("configure", matches)) => {
            let provider = matches
                .get_one::<String>("provider")
                .ok_or(eyre::eyre!("No provider specified"))?;

            // Check if the input provider is valid
            let provider = Provider::from_str(provider)?;

            // If yes, set the provider in the config
            config.provider = provider;

            // And save the config
            config.save()
        }
        Some(("get", matches)) => {
            let address = matches
                .get_one::<String>("address")
                .ok_or(eyre::eyre!("No address specified"))?;
            let date = matches
                .get_one::<String>("date")
                .cloned()
                .unwrap_or("now".to_string());

            // Get the weather data
            let data = config.provider.get(address, date)?;

            // Draw the weather data
            draw_data(data)
        }
        _ => Ok(()),
    }
}
