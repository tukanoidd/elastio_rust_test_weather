use std::fmt::{Display, Formatter};

use color_eyre::eyre;
use geocoding::{Forward, Openstreetmap, Point, Reverse};
use itertools::Itertools;

use crate::data::WeatherData;

/// These providers are free and don't require an API key.
/// I chose them deliberately because of security concerns of having API keys that are
/// tied to my account and my wallet available in a public repo

macro_rules! decl_provider_enum {
    ($len:literal: [$($variant:ident => $str:literal),*]) => {
        #[derive(
            Default, Debug, Copy, Clone, enum_iterator::Sequence, serde::Serialize, serde::Deserialize,
        )]
        #[serde(rename_all = "snake_case")]
        pub(crate) enum Provider {
            #[default]
            $($variant),*
        }

        impl Display for Provider {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(Self::$variant => write!(f, $str)),*
                }
            }
        }

        impl Provider {
            pub(crate) const AVAILABLE_PROVIDERS: [&str; $len] = [$($str),*];

            /// Parse a string into a provider
            pub(crate) fn from_str(s: impl AsRef<str>) -> eyre::Result<Self> {
                match s.as_ref() {
                    $($str => Ok(Self::$variant),)*
                    _ => Err(eyre::eyre!(
                        r"
                            Invalid provider!
                            Available providers: [{}]
                            ",
                        Self::AVAILABLE_PROVIDERS.iter().join(", ")
                    ))
                }
            }
        }
    };
}

decl_provider_enum!(2: [OpenMeteo => "open_meteo", MetNo => "met_no"]);

impl Provider {
    /// Get the weather data for a given address and a date
    pub(crate) fn get(&self, address: impl AsRef<str>, date: String) -> eyre::Result<WeatherData> {
        // Create the request builder and set the address
        let mut request_builder = ProviderRequestBuilder::new(*self).address(address)?;

        // Check which provider we are using
        request_builder = match self {
            // If we're using open_meteo, just set the date, as it supports custom dates
            Provider::OpenMeteo => request_builder.date(date)?,
            // If we're using met_no, check if the date is "now"
            Provider::MetNo => match date.as_str() == "now" {
                // If it is, just set the date
                true => request_builder.date(date)?,
                // But if it isn't, return an error
                false => {
                    return Err(eyre::eyre!("met_no doesn't support custom dates"));
                }
            },
        };

        // Build and execute the request
        request_builder.execute()
    }

    /// API base URL
    fn base_url(&self) -> &'static str {
        match self {
            Provider::OpenMeteo => "https://api.open-meteo.com/v1",
            Provider::MetNo => "https://api.met.no/weatherapi/locationforecast/2.0",
        }
    }

    /// API parameter format for latitude value
    fn latitude_param(&self) -> &'static str {
        match self {
            Provider::OpenMeteo => "latitude",
            Provider::MetNo => "lat",
        }
    }

    /// API parameter format for longitude value
    fn longitude_param(&self) -> &'static str {
        match self {
            Provider::OpenMeteo => "longitude",
            Provider::MetNo => "lon",
        }
    }

    /// API parameter format for date value
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
    /// Parameters that are added to the request URL
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

    /// Set the address
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

                // Check if the vector has two elements and if they are both valid floats
                let lat_lon_f64 = match parts.len() == 2 {
                    true => {
                        let lat = parts[0].parse::<f64>().ok();
                        let lon = parts[1].parse::<f64>().ok();

                        lat.and_then(|lat| lon.map(|lon| (lat, lon)))
                    }
                    false => None,
                };

                // If yes, we got the latitude and longitude
                match lat_lon_f64 {
                    Some((lat, lon)) => {
                        if lat < -90.0 || lat > 90.0 {
                            return Err(eyre::eyre!("Latitude must be between -90 and 90"));
                        }

                        if lon < -180.0 || lon > 180.0 {
                            return Err(eyre::eyre!("Longitude must be between -180 and 180"));
                        }

                        Some((lat.to_string(), lon.to_string()))
                    }
                    None => None,
                }
            }
            false => None,
        };

        let osm = Openstreetmap::new();

        let lat_lon = match maybe_lat_lon {
            // If lat, lnog were not provided as the address
            None => {
                self.address = address.as_ref().to_string();

                // Try to get the coordinates from the address
                let points = osm.forward(address.as_ref())?;
                let lon_lat_point: &Point<f64> = points
                    .first()
                    .ok_or(eyre::eyre!("Could not find location"))?;

                (lon_lat_point.y().to_string(), lon_lat_point.x().to_string())
            }
            Some(lat_lon) => {
                // If lat, lon were provided as the address, parse them to doubles
                let lat = lat_lon.0.parse::<f64>()?;
                let lon = lat_lon.1.parse::<f64>()?;

                // General writing convention for coordinates seems to be lat long from just
                // browsing the net, but the api here requires lon lat, so thats why im swapping
                // them like this
                let lon_lat_point = Point::<f64>::new(lon, lat);

                // Search for an save the address that we get from coordinates provided
                self.address = osm
                    .reverse(&lon_lat_point)
                    .map_err(|e| eyre::eyre!("Couldn't reverse the (lon, lat) to an address: {e}"))?
                    .ok_or(eyre::eyre!("Could not find location"))?;

                lat_lon
            }
        };

        // Add the latitude and longitude to the parameters list
        self.params
            .push(format!("{}={}", self.provider.latitude_param(), lat_lon.0));
        self.params
            .push(format!("{}={}", self.provider.longitude_param(), lat_lon.1));

        Ok(self)
    }

    /// Set the date
    fn date(mut self, date: String) -> eyre::Result<Self> {
        // Parse the date string to local NaiveDateTime and check if it refers to "now" or not
        let (date_time, now) = match date.as_str() {
            "now" => (chrono::Utc::now().naive_local(), true),
            _ => {
                let parsed_date = dateparser::parse(&date)
                    .map_err(|e| eyre::eyre!("Couldn't parse the date: {e}"))?;

                (parsed_date.naive_local(), false)
            }
        };

        // Save the date as a string with the specific format used in UI
        self.requested_date = date_time.format("%Y-%m-%d").to_string();

        // Set the request type based on the date
        self.request_type = match now {
            // If it's "now", it's a forecast
            true => ProviderRequestType::Forecast,
            false => match date_time < chrono::Utc::now().naive_local() {
                // If it's before "now", it's a history
                true => ProviderRequestType::History,
                // If it's after "now", it's a forecast
                false => ProviderRequestType::Forecast,
            },
        };

        // Check which provider is being used
        match self.provider {
            Provider::OpenMeteo => {
                // Construct the date string
                let date_str = date_time.format(self.provider.date_format()?).to_string();

                // Add the appropriate parameters to the request
                self.params.push(format!("start_date={}", date_str));
                self.params.push(format!("end_date={}", date_str));
            }
            Provider::MetNo => {
                // If it's met_no provider and the date is still somehow custom, throw an error
                if !now {
                    return Err(eyre::eyre!(
                        "Custom dates (including history) are not supported by met_no provider"
                    ));
                }
            }
        }

        Ok(self)
    }

    // Build and execute the request
    fn execute(mut self) -> eyre::Result<WeatherData> {
        // Check which provider is being used to add additional parameters in case they are needed
        match self.provider {
            Provider::OpenMeteo => {
                // If it's open_meteo and the request type is forecast, it means that we can also
                // ask for current weather conditions from the endpoint
                if matches!(self.request_type, ProviderRequestType::Forecast) {
                    self.params.push("current_weather=true".to_string());
                }

                // Add the parameter to the get hourly forecast
                self.params.push("hourly=temperature_2m".to_string());
            }
            Provider::MetNo => {}
        }

        // Construct the request string
        let request_str = format!(
            "{}/{}?{}",
            self.provider.base_url(),
            self.request_type.to_string(&self.provider)?,
            self.params.join("&")
        );

        // Check which provider is being used, execute the request based on the provider and get the
        // json data from the response
        let json = match self.provider {
            // If it's open_meteo, just use normal get request
            Provider::OpenMeteo => reqwest::blocking::get(request_str)?.json()?,
            // For met_no, we need to specify some headers, so here I'm using Client to build the
            // appropriate request
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

        // Parse the json data to WeatherData struct
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
