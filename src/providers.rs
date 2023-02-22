use std::fmt::{Display, Formatter};

use color_eyre::eyre;
use geocoding::{Forward, Openstreetmap, Point, Reverse};

use crate::data::WeatherData;

/// These providers are free and don't require an API key.
/// I chose them deliberately because of security concerns of having API keys that are
/// tied to my account and my wallet available in a public repo
#[derive(
    Default, Debug, Copy, Clone, enum_iterator::Sequence, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Provider {
    #[default]
    OpenMeteo,
}

impl Display for Provider {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenMeteo => write!(f, "open_meteo"),
        }
    }
}

impl Provider {
    pub(crate) fn from_str(s: impl AsRef<str>) -> eyre::Result<Self> {
        enum_iterator::all::<Self>()
            .find(|p| p.to_string() == s.as_ref())
            .ok_or(eyre::eyre!(
                r"
            Invalid provider!
            Available providers: [{}]
            ",
                Self::available_providers().join(", ")
            ))
    }

    pub(crate) fn available_providers() -> Vec<String> {
        enum_iterator::all::<Self>()
            .map(|p| p.to_string())
            .collect()
    }

    pub(crate) fn get(&self, address: impl AsRef<str>, date: String) -> eyre::Result<WeatherData> {
        let request_builder = ProviderRequestBuilder::new(*self)
            .address(address)?
            .date(date)?;

        request_builder.execute()
    }

    fn base_url(&self) -> &'static str {
        match self {
            Provider::OpenMeteo => "https://api.open-meteo.com/v1",
        }
    }

    fn latitude_param(&self) -> &'static str {
        match self {
            Provider::OpenMeteo => "latitude",
        }
    }

    fn longitude_param(&self) -> &'static str {
        match self {
            Provider::OpenMeteo => "longitude",
        }
    }

    fn date_format(&self) -> &'static str {
        match self {
            Provider::OpenMeteo => "%Y-%m-%d",
        }
    }
}

#[derive(Default, Debug)]
pub(crate) enum ProviderRequestType {
    #[default]
    Forecast,
    History,
}

impl ProviderRequestType {
    fn to_string(&self, provider: &Provider) -> &'static str {
        match self {
            ProviderRequestType::Forecast => match provider {
                Provider::OpenMeteo => "forecast",
            },
            ProviderRequestType::History => match provider {
                Provider::OpenMeteo => "archive",
            },
        }
    }
}

struct ProviderRequestBuilder {
    provider: Provider,
    requested_date: String,
    address: String,
    params: Vec<String>,
    request_type: ProviderRequestType,
}

impl ProviderRequestBuilder {
    fn new(provider: Provider) -> Self {
        Self {
            provider,
            requested_date: String::new(),
            address: "Unknown".to_string(),
            params: Vec::new(),
            request_type: ProviderRequestType::Forecast,
        }
    }

    fn address(mut self, address: impl AsRef<str>) -> eyre::Result<Self> {
        // Check if the address contains a comma
        let maybe_lon_lat = match address.as_ref().contains(',') {
            true => {
                // If it does, split it into a vector of separated strings
                let parts = address
                    .as_ref()
                    .split(',')
                    .map(|s| s.trim())
                    .collect::<Vec<_>>();

                let are_lon_lat =
                    parts.len() == 2 && parts.iter().all(|p| p.parse::<f64>().is_ok());

                match are_lon_lat {
                    true => Some((parts[0].to_string(), parts[1].to_string())),
                    false => None,
                }
            }
            false => None,
        };

        let osm = Openstreetmap::new();

        let lon_lat = match maybe_lon_lat {
            None => {
                self.address = address.as_ref().to_string();

                let points = osm.forward(address.as_ref())?;
                let lon_lat_point: &Point<f64> = points
                    .first()
                    .ok_or(eyre::eyre!("Could not find location"))?;

                (lon_lat_point.x().to_string(), lon_lat_point.y().to_string())
            }
            Some(lon_lat) => {
                self.address = osm
                    .reverse(&Point::<f64>::new(lon_lat.0.parse()?, lon_lat.1.parse()?))?
                    .ok_or(eyre::eyre!("Could not find location"))?;

                lon_lat
            }
        };

        self.params
            .push(format!("{}={}", self.provider.latitude_param(), lon_lat.1));
        self.params
            .push(format!("{}={}", self.provider.longitude_param(), lon_lat.0));

        Ok(self)
    }

    fn date(mut self, date: String) -> eyre::Result<Self> {
        let (date_time, now) = match date.as_str() {
            "now" => (chrono::Utc::now().naive_local(), true),
            _ => {
                let parsed_date = dateparser::parse(&date)
                    .map_err(|e| eyre::eyre!("Couldn't parse the date: {e}"))?;

                (parsed_date.naive_local(), false)
            }
        };

        self.requested_date = date_time.format("%Y-%m-%d").to_string();

        self.request_type = match now {
            true => ProviderRequestType::Forecast,
            false => match date_time < chrono::Utc::now().naive_local() {
                true => ProviderRequestType::History,
                false => ProviderRequestType::Forecast,
            },
        };

        let date_str = date_time.format(self.provider.date_format()).to_string();

        match self.provider {
            Provider::OpenMeteo => {
                self.params.push(format!("start_date={}", date_str));
                self.params.push(format!("end_date={}", date_str));
            }
        }

        Ok(self)
    }

    fn execute(mut self) -> eyre::Result<WeatherData> {
        match self.provider {
            Provider::OpenMeteo => {
                if matches!(self.request_type, ProviderRequestType::Forecast) {
                    self.params.push("current_weather=true".to_string());
                }

                self.params.push("hourly=temperature_2m".to_string());
            }
        }

        let request_str = format!(
            "{}/{}?{}",
            self.provider.base_url(),
            self.request_type.to_string(&self.provider),
            self.params.join("&")
        );

        println!("{request_str}");

        let json = reqwest::blocking::get(request_str)?.json()?;

        let data = WeatherData::from_json(
            &json,
            self.provider,
            self.request_type,
            self.requested_date,
            self.address,
        )?;

        Ok(data)
    }
}
