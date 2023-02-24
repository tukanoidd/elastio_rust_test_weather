use std::fmt::{Display, Formatter};

use color_eyre::eyre;
use geocoding::{Forward, Openstreetmap, Point, Reverse};
use itertools::Itertools;
use serde_json::{Map, Value};

use crate::data::WeatherData;

/// These providers are free and don't require an API key.
/// I chose them deliberately because of security concerns of having API keys that are
/// tied to my account and my wallet available in a public repo

macro_rules! decl_provider_enum {
    ($len:literal: [$(
        $variant:ident => (
            str: $str:literal,
            base_url: $base_url:literal,
            lat_param: $lat_param:literal,
            lon_param: $lon_param:literal
        )
    ),*]) => {
        #[derive(
            Default, Debug, Copy, Clone,
            enum_iterator::Sequence, serde::Serialize, serde::Deserialize,
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

            /// API base URL
            fn base_url(&self) -> &'static str {
                match self {
                    $(Self::$variant => $base_url),*
                }
            }

            /// API parameter format for latitude value
            fn lat_param(&self) -> &'static str {
                match self {
                    $(Self::$variant => $lat_param),*
                }
            }

            /// API parameter format for longitude value
            fn lon_param(&self) -> &'static str {
                match self {
                    $(Self::$variant => $lon_param),*
                }
            }
        }
    };
}

decl_provider_enum!(2: [
    OpenMeteo => (
        str: "open_meteo",
        base_url: "https://api.open-meteo.com/v1",
        lat_param: "latitude",
        lon_param: "longitude"
    ),
    MetNo => (
        str: "met_no",
        base_url: "https://api.met.no/weatherapi/locationforecast/2.0",
        lat_param: "lat",
        lon_param: "lon"
    )
]);

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
        let (request_str, request_type, requested_date, address) = request_builder.build()?;

        // Check which provider is being used, execute the request based on the provider and get the
        // json data from the response
        let json = self.request(request_str)?;

        // Parse the json data to WeatherData struct
        let data = WeatherData::from_json(&json, *self, request_type, requested_date, address)?;

        Ok(data)
    }

    fn request(&self, request_str: impl reqwest::IntoUrl) -> eyre::Result<Map<String, Value>> {
        match self {
            // If it's open_meteo, just use normal get request
            Provider::OpenMeteo => Ok(reqwest::blocking::get(request_str)?.json()?),
            // For met_no, we need to specify some headers, so here I'm using Client to build the
            // appropriate request
            Provider::MetNo => {
                let client = reqwest::blocking::Client::new();
                let response = client
                    .get(request_str)
                    .header("Accept", "application/json")
                    .header("User-Agent", "tukweathercli/0.1.0")
                    .send()?;

                Ok(response.json()?)
            }
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
                        if !(-90.0..=90.0).contains(&lat) {
                            return Err(eyre::eyre!("Latitude must be between -90 and 90"));
                        }

                        if !(-180.0..=180.0).contains(&lon) {
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
            // If lat, lon were not provided as the address
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
            .push(format!("{}={}", self.provider.lat_param(), lat_lon.0));
        self.params
            .push(format!("{}={}", self.provider.lon_param(), lat_lon.1));

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

    /// Build the request string and return the relevant data collected during configuration phase
    fn build(mut self) -> eyre::Result<(String, ProviderRequestType, String, String)> {
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

        Ok((
            request_str,
            self.request_type,
            self.requested_date,
            self.address,
        ))
    }
}
