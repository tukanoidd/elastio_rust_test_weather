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
    MetNo,
}

impl Display for Provider {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenMeteo => write!(f, "open_meteo"),
            Provider::MetNo => write!(f, "met_no"),
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
        let mut request_builder = ProviderRequestBuilder::new(*self).address(address)?;

        request_builder = match self {
            Provider::OpenMeteo => request_builder.date(date)?,
            Provider::MetNo => match date.as_str() == "now" {
                true => request_builder.date(date)?,
                false => {
                    return Err(eyre::eyre!("met_no doesn't support custom dates"));
                }
            },
        };

        request_builder.execute()
    }

    fn base_url(&self) -> &'static str {
        match self {
            Provider::OpenMeteo => "https://api.open-meteo.com/v1",
            Provider::MetNo => "https://api.met.no/weatherapi/locationforecast/2.0",
        }
    }

    fn latitude_param(&self) -> &'static str {
        match self {
            Provider::OpenMeteo => "latitude",
            Provider::MetNo => "lat",
        }
    }

    fn longitude_param(&self) -> &'static str {
        match self {
            Provider::OpenMeteo => "longitude",
            Provider::MetNo => "lon",
        }
    }

    fn date_format(&self) -> eyre::Result<&'static str> {
        match self {
            Provider::OpenMeteo => Ok("%Y-%m-%d"),
            Provider::MetNo => Err(eyre::eyre!("met_no doesn't support custom dates")),
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
    fn to_string(&self, provider: &Provider) -> eyre::Result<&'static str> {
        match self {
            ProviderRequestType::Forecast => Ok(match provider {
                Provider::OpenMeteo => "forecast",
                Provider::MetNo => "complete",
            }),
            ProviderRequestType::History => match provider {
                Provider::OpenMeteo => Ok("archive"),
                Provider::MetNo => Err(eyre::eyre!("History is not supported by met_no provider")),
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
        let maybe_lat_lon = match address.as_ref().contains(',') {
            true => {
                // If it does, split it into a vector of separated strings
                let parts = address
                    .as_ref()
                    .split(',')
                    .map(|s| s.trim())
                    .collect::<Vec<_>>();

                let are_lat_lon =
                    parts.len() == 2 && parts.iter().all(|p| p.parse::<f64>().is_ok());

                match are_lat_lon {
                    true => Some((parts[0].to_string(), parts[1].to_string())),
                    false => None,
                }
            }
            false => None,
        };

        let osm = Openstreetmap::new();

        let lat_lon = match maybe_lat_lon {
            None => {
                self.address = address.as_ref().to_string();

                let points = osm.forward(address.as_ref())?;
                let lon_lat_point: &Point<f64> = points
                    .first()
                    .ok_or(eyre::eyre!("Could not find location"))?;

                (lon_lat_point.y().to_string(), lon_lat_point.x().to_string())
            }
            Some(lat_lon) => {
                let lat = lat_lon.0.parse::<f64>()?;
                let lon = lat_lon.1.parse::<f64>()?;

                // General writing convention for coordinates seems to be lat long from just
                // browsing the net, but the api here requires lon lat, so thats why im swapping
                // them liek this
                let lon_lat_point = Point::<f64>::new(lon, lat);

                self.address = osm
                    .reverse(&lon_lat_point)
                    .map_err(|e| eyre::eyre!("Couldn't reverse the (lon, lat) to an address: {e}"))?
                    .ok_or(eyre::eyre!("Could not find location"))?;

                lat_lon
            }
        };

        self.params
            .push(format!("{}={}", self.provider.latitude_param(), lat_lon.0));
        self.params
            .push(format!("{}={}", self.provider.longitude_param(), lat_lon.1));

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

        match self.provider {
            Provider::OpenMeteo => {
                let date_str = date_time.format(self.provider.date_format()?).to_string();

                self.params.push(format!("start_date={}", date_str));
                self.params.push(format!("end_date={}", date_str));
            }
            Provider::MetNo => {
                if !now {
                    return Err(eyre::eyre!(
                        "Custom dates (including history) are not supported by met_no provider"
                    ));
                }
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
            Provider::MetNo => {}
        }

        let request_str = format!(
            "{}/{}?{}",
            self.provider.base_url(),
            self.request_type.to_string(&self.provider)?,
            self.params.join("&")
        );

        let json = match self.provider {
            Provider::OpenMeteo => reqwest::blocking::get(request_str)?.json()?,
            Provider::MetNo => {
                let client = reqwest::blocking::Client::new();
                let response = client
                    .get(request_str)
                    .header("Accept", "application/json")
                    .header("User-Agent", "tukweathercli/0.1.0")
                    .send()?;

                response.json()?
            }
        };

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
