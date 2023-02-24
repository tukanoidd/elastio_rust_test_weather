use std::fmt::{Display, Formatter};

use color_eyre::eyre;
use itertools::{
    FoldWhile::{Continue, Done},
    Itertools,
};
use serde_json::{Map, Value};

use crate::providers::{Provider, ProviderRequestType};

#[derive(Default, Debug)]
pub(crate) struct WeatherData {
    pub(crate) provider: Provider,
    pub(crate) request_type: ProviderRequestType,

    pub(crate) requested_date: String,
    pub(crate) address: String,

    pub(crate) latitude: f64,
    pub(crate) longitude: f64,

    pub(crate) timestamps: Vec<String>,
    pub(crate) temperatures: Vec<f64>,
    pub(crate) unit: String,

    pub(crate) current: Option<CurrentWeatherData>,
}

impl WeatherData {
    pub(crate) fn from_json(
        json: &Map<String, Value>,
        provider: Provider,
        request_type: ProviderRequestType,
        requested_date: String,
        address: String,
    ) -> eyre::Result<Self> {
        let res = Self {
            provider,
            request_type,
            requested_date,
            address,
            ..Default::default()
        };

        // Parse the json based on the provider
        match &res.provider {
            Provider::OpenMeteo => res.parse_open_meteo_json(json),
            Provider::MetNo => res.parse_met_no_json(json),
        }
    }

    fn parse_open_meteo_json(mut self, json: &Map<String, Value>) -> eyre::Result<Self> {
        if let (Some(Value::Bool(true)), Some(Value::String(reason))) =
            (json.get("error"), json.get("reason"))
        {
            return Err(eyre::eyre!("Error response from open_meteo: {}", reason));
        }

        self.latitude = json
            .get("latitude")
            .and_then(|l| l.as_f64())
            .ok_or(eyre::eyre!("Latitude not found"))?;
        self.longitude = json
            .get("longitude")
            .and_then(|l| l.as_f64())
            .ok_or(eyre::eyre!("Longitude not found"))?;

        (self.timestamps, self.temperatures) = {
            let hourly = json
                .get("hourly")
                .ok_or(eyre::eyre!("Hourly data not found"))?;

            match hourly {
                Value::Object(hourly) => {
                    let time = hourly.get("time").ok_or(eyre::eyre!("Time not found"))?;

                    let timestamps = match time {
                        Value::Array(time) => {
                            let timestamps = time
                                .clone()
                                .into_iter()
                                .map(|t| t.as_str().map(|t| t.replace('T', " ")))
                                .collect_vec();

                            // If any of the timestamps couldn't be parsed, return an error
                            match timestamps.iter().any(|t| t.is_none()) {
                                true => Err(eyre::eyre!("Couldn't parse timestamps")),
                                false => {
                                    let mapped_timestamps = timestamps
                                        .into_iter()
                                        .flatten() // We can fearlessly flatten here since we already checked for nulls in the match
                                        .map_while(|t| {
                                            let date = match dateparser::parse(&t) {
                                                Ok(date) => date,
                                                Err(err) => {
                                                    panic!(
                                                        "Couldn't parse timestamp ({t}): {}",
                                                        err
                                                    )
                                                }
                                            };

                                            Some(date.format("%I %p").to_string())
                                        })
                                        .collect_vec();

                                    match mapped_timestamps.len() == time.len() {
                                        true => Ok(mapped_timestamps),
                                        false => {
                                            Err(eyre::eyre!("Couldn't reformat all the timestamps"))
                                        }
                                    }
                                }
                            }
                        }
                        _ => Err(eyre::eyre!("Couldn't parse timestamps")),
                    }?;

                    let temperatures = {
                        let temperature = hourly
                            .get("temperature_2m")
                            .ok_or(eyre::eyre!("Temperature not found"))?;

                        match temperature {
                            Value::Array(temperature) => {
                                let temperatures = temperature
                                    .clone()
                                    .into_iter()
                                    .map(|t| t.as_f64())
                                    .collect_vec();

                                match temperatures.iter().any(|t| t.is_none()) {
                                    true => Err(eyre::eyre!("Couldn't parse temperatures")),
                                    false => Ok(temperatures.into_iter().flatten().collect_vec()),
                                }
                            }
                            _ => Err(eyre::eyre!("Couldn't parse temperatures")),
                        }
                    }?;

                    match timestamps.len() == temperatures.len() {
                        true => Ok((timestamps, temperatures)),
                        false => Err(eyre::eyre!("Mismatch in timestamps and temperatures data, please try a different provider/location/date")),
                    }
                }
                _ => Err(eyre::eyre!("Couldn't parse hourly data")),
            }?
        };

        self.unit = {
            let unit = json
                .get("hourly_units")
                .ok_or(eyre::eyre!("Unit not found"))?;

            unit.get("temperature_2m")
                .and_then(|u| u.as_str())
                .ok_or(eyre::eyre!("Unit not found"))?
                .to_string()
        };

        self.current = {
            let current_weather = json
                .get("current_weather")
                .ok_or(eyre::eyre!("Current weather not found"))?;

            match current_weather {
                Value::Object(current_weather) => {
                    let current_weather = CurrentWeatherData::from_json(current_weather)?;
                    Ok(Some(current_weather))
                }
                _ => Err(eyre::eyre!("Couldn't parse current weather data")),
            }?
        };

        Ok(self)
    }

    fn parse_met_no_json(mut self, json: &Map<String, Value>) -> eyre::Result<Self> {
        let Value::Array(coords_arr) = json
            .get("geometry")
            .ok_or(eyre::eyre!("Geometry not found"))?
            .get("coordinates")
            .ok_or(eyre::eyre!("Coordinates not found"))? else {
            return Err(eyre::eyre!("Couldn't parse coordinates"));
        };

        if coords_arr.len() < 2 {
            return Err(eyre::eyre!("Couldn't parse coordinates"));
        }

        self.latitude = coords_arr[1]
            .as_f64()
            .ok_or(eyre::eyre!("Couldn't parse latitude"))?;
        self.longitude = coords_arr[0]
            .as_f64()
            .ok_or(eyre::eyre!("Couldn't parse longitude"))?;

        let properties = json
            .get("properties")
            .ok_or(eyre::eyre!("Properties not found"))?;

        self.unit = properties
            .get("meta")
            .and_then(|m| m.get("units"))
            .and_then(|u| u.get("air_temperature"))
            .and_then(|t| t.as_str())
            .ok_or(eyre::eyre!("Couldn't parse unit"))?
            .to_string();

        let Value::Array(time_series) = properties
            .get("timeseries")
            .ok_or(eyre::eyre!("Timeseries not found"))? else {
            return Err(eyre::eyre!("Couldn't parse timeseries"));
        };

        let time_series = time_series.iter().take(24).collect_vec();

        let (timestamps, temperatures, err) = time_series
            .into_iter()
            .fold_while(
                (Vec::new(), Vec::new(), None),
                |(mut ts, mut temps, _), map| {
                    let timestep = match map
                        .get("time")
                        .ok_or("Couldn't find time field".to_string())
                        .and_then(|t| {
                            t.as_str()
                                .map(|t| t.replace('T', " ").replace('Z', ""))
                                .ok_or("time field is not a string".to_string())
                        })
                        .and_then(|t| {
                            let date = match dateparser::parse(&t) {
                                Ok(date) => date,
                                Err(err) => {
                                    return Err(format!("Couldn't parse timestamp ({t}): {err}"));
                                }
                            };

                            Ok(date.format("%I %p").to_string())
                        }) {
                        Ok(timestep) => timestep,
                        Err(err) => return Done((ts, temps, Some(err))),
                    };

                    ts.push(timestep);

                    let temperature = match map
                        .get("data")
                        .ok_or("Couldn't find data field")
                        .and_then(|d| d.get("instant").ok_or("Couldn't find instant field"))
                        .and_then(|i| i.get("details").ok_or("Couldn't find details field"))
                        .and_then(|d| {
                            d.get("air_temperature")
                                .ok_or("Couldn't find air_temperature_field")
                        })
                        .and_then(|a| a.as_f64().ok_or("Couldn't parse air_temperature"))
                    {
                        Ok(temperature) => temperature,
                        Err(err) => return Done((ts, temps, Some(err.to_string()))),
                    };

                    temps.push(temperature);

                    Continue((ts, temps, None))
                },
            )
            .into_inner();

        (self.timestamps, self.temperatures) = match err {
            Some(err) => return Err(eyre::eyre!(err)),
            None => (timestamps, temperatures),
        };

        Ok(self)
    }
}

#[derive(Debug)]
pub(crate) struct CurrentWeatherData {
    pub(crate) time: String,
    pub(crate) temperature: f64,
    pub(crate) weather_code: WeatherCode,
    pub(crate) wind_speed: f64,
    pub(crate) wind_speed_unit: String,
    pub(crate) wind_direction: WindDirection,
}

impl CurrentWeatherData {
    fn from_json(json: &Map<String, Value>) -> eyre::Result<Self> {
        let time = json
            .get("time")
            .and_then(|t| t.as_str().map(|t| t.replace('T', " ")))
            .ok_or(eyre::eyre!("Time not found"))?;

        let temperature = json
            .get("temperature")
            .and_then(|t| t.as_f64())
            .ok_or(eyre::eyre!("Temperature not found"))?;

        let weather_code = json
            .get("weathercode")
            .and_then(|t| t.as_u64().map(WeatherCode::from_open_meteo))
            .ok_or(eyre::eyre!("Weather code not found"))?;

        let wind_speed = json
            .get("windspeed")
            .and_then(|t| t.as_f64())
            .ok_or(eyre::eyre!("Wind speed not found"))?;

        let wind_direction = json
            .get("winddirection")
            .and_then(|t| t.as_f64().map(WindDirection::from_degrees))
            .ok_or(eyre::eyre!("Wind direction not found"))?;

        Ok(Self {
            time,
            temperature,
            weather_code,
            wind_speed,
            wind_speed_unit: "km/h".to_string(),
            wind_direction,
        })
    }
}

#[derive(Default, Debug)]
pub(crate) enum WeatherCode {
    #[default]
    Unknown,
    ClearSky,
    MainlyClear,
    PartlyCloudy,
    Overcast,
    Fog,
    Drizzle,
    FreezingDrizzle,
    Rain,
    FreezingRain,
    SnowFall,
    SnowGrains,
    RainShowers,
    SnowShowers,
    Thunderstorm,
}

impl Display for WeatherCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            WeatherCode::Unknown => write!(f, "Unknown"),
            WeatherCode::ClearSky => write!(f, "Clear sky"),
            WeatherCode::MainlyClear => write!(f, "Mainly Clear Sky"),
            WeatherCode::PartlyCloudy => write!(f, "Partly Cloudy"),
            WeatherCode::Overcast => write!(f, "Overcast"),
            WeatherCode::Fog => write!(f, "Fog"),
            WeatherCode::Drizzle => write!(f, "Drizzle"),
            WeatherCode::FreezingDrizzle => write!(f, "Freezing Drizzle"),
            WeatherCode::Rain => write!(f, "Rain"),
            WeatherCode::FreezingRain => write!(f, "Freezing Rain"),
            WeatherCode::SnowFall => write!(f, "Snow Fall"),
            WeatherCode::SnowGrains => write!(f, "Snow Grains"),
            WeatherCode::RainShowers => write!(f, "Rain Showers"),
            WeatherCode::SnowShowers => write!(f, "Snow Showers"),
            WeatherCode::Thunderstorm => write!(f, "Thunderstorm"),
        }
    }
}

impl WeatherCode {
    fn from_open_meteo(code: u64) -> Self {
        match code {
            0 => WeatherCode::ClearSky,
            1 => WeatherCode::MainlyClear,
            2 => WeatherCode::PartlyCloudy,
            3 => WeatherCode::Overcast,
            45 | 48 => WeatherCode::Fog,
            51 | 53 | 55 => WeatherCode::Drizzle,
            56 | 57 => WeatherCode::FreezingDrizzle,
            61 | 63 | 65 => WeatherCode::Rain,
            66 | 67 => WeatherCode::FreezingRain,
            71 | 73 | 75 => WeatherCode::SnowFall,
            77 => WeatherCode::SnowGrains,
            80 | 81 | 82 => WeatherCode::RainShowers,
            85 | 86 => WeatherCode::SnowShowers,
            95 | 96 | 99 => WeatherCode::Thunderstorm,
            _ => WeatherCode::Unknown,
        }
    }
}

type DegreeRanges = (Option<(f64, f64)>, Option<(f64, f64)>);

macro_rules! deg_ranges {
    ((n, n)) => {
        (None, None)
    };
    ((n, ($s1:literal, $s2:literal))) => {
        (None, Some(($s1, $s2)))
    };
    ((($s1:literal, $s2:literal), n)) => {
        (Some(($s1, $s2)), None)
    };
    ((($s1:literal, $s2:literal), ($s3:literal, $s4:literal))) => {
        (Some(($s1, $s2)), Some(($s3, $s4)))
    };
}

macro_rules! wind_direction_decl {
    ($len:literal : [$(
        $variant:ident => (
            str: $str:literal,
            deg_ranges: $tt:tt
        )
    ),*]) => {
        #[allow(clippy::upper_case_acronyms)]
        #[derive(Default, Debug)]
        pub(crate) enum WindDirection {
            #[default]
            $($variant),*
        }

        impl Display for WindDirection {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(Self::$variant => write!(f, $str)),*
                }
            }
        }

        impl WindDirection {
            const WIND_DIRECTIONS: [Self; $len] = [$(Self::$variant),*];

            /// http://snowfence.umn.edu/Components/winddirectionanddegrees.htm
            fn degree_ranges(&self) -> DegreeRanges {
                match self {
                    $(Self::$variant => deg_ranges!($tt)),*
                }
            }
        }
    };
}

wind_direction_decl!(17: [
    Unknown => (str: "Unknown",deg_ranges: (n, n)),
    N => (str: "N", deg_ranges: ((0.0, 11.25), (348.75, 360.0))),
    NNE => (str: "NNE", deg_ranges: ((11.25, 33.75), n)),
    NE => (str: "NE", deg_ranges: ((33.75, 56.25), n)),
    ENE => (str: "ENE", deg_ranges: ((56.25, 78.75), n)),
    E => (str: "E", deg_ranges: ((78.75, 101.25), n)),
    ESE => (str: "ESE", deg_ranges: ((101.25, 123.75), n)),
    SE => (str: "SE", deg_ranges: ((123.75, 146.25), n)),
    SSE => (str: "SSE", deg_ranges: ((146.25, 168.75), n)),
    S => (str: "S", deg_ranges: ((168.75, 191.25), n)),
    SSW => (str: "SSW", deg_ranges: ((191.25, 213.75), n)),
    SW => (str: "SW", deg_ranges: ((213.75, 236.25), n)),
    WSW => (str: "WSW", deg_ranges: ((236.25, 258.75), n)),
    W => (str: "W", deg_ranges: ((258.75, 281.25), n)),
    WNW => (str: "WNW", deg_ranges: ((281.25, 303.75), n)),
    NW => (str: "NW", deg_ranges: ((303.75, 326.25), n)),
    NNW => (str: "NNW", deg_ranges: ((326.25, 348.75), n))
]);

impl WindDirection {
    fn from_degrees(degrees: f64) -> Self {
        let deg = (degrees % 360.0).round();

        Self::WIND_DIRECTIONS
            .into_iter()
            .find_or_first(|dir| {
                let (min_max, opt_min_max) = dir.degree_ranges();

                match (min_max, opt_min_max) {
                    (Some((min, max)), None) => min <= deg && deg <= max,
                    (Some((min, max)), Some((min2, max2))) => {
                        (min <= deg && deg <= max) || (min2 <= deg && deg <= max2)
                    }
                    _ => false,
                }
            })
            .unwrap() // We definitely know that the list of enum variants is not empty, so we can unwrap here
    }
}
