use std::fmt::{Display, Formatter};

use color_eyre::eyre;
use enum_iterator::Sequence;
use itertools::Itertools;
use serde_json::{Map, Value};

use crate::providers::Provider;

#[derive(Default, Debug)]
pub(crate) struct WeatherData {
    provider: Provider,

    latitude: f64,
    longitude: f64,
    elevation: f64,
    timezone: String,
    timezone_abbreviation: String,

    timestamps: Vec<String>,
    temperatures: Vec<f64>,
    unit: String,

    current: Option<CurrentWeatherData>,
}

impl WeatherData {
    pub(crate) fn from_json(json: &Map<String, Value>, provider: Provider) -> eyre::Result<Self> {
        let mut res = Self {
            provider,
            ..Default::default()
        };

        match &res.provider {
            Provider::OpenMeteo => res.parse_open_meteo_json(json),
        }
    }

    fn parse_open_meteo_json(mut self, json: &Map<String, Value>) -> eyre::Result<Self> {
        self.latitude = json
            .get("latitude")
            .and_then(|l| l.as_f64())
            .ok_or(eyre::eyre!("Latitude not found"))?;
        self.longitude = json
            .get("longitude")
            .and_then(|l| l.as_f64())
            .ok_or(eyre::eyre!("Longitude not found"))?;
        self.elevation = json
            .get("elevation")
            .and_then(|l| l.as_f64())
            .ok_or(eyre::eyre!("Elevation not found"))?;
        self.timezone = json
            .get("timezone")
            .and_then(|l| l.as_str())
            .ok_or(eyre::eyre!("Timezone not found"))?
            .to_string();
        self.timezone_abbreviation = json
            .get("timezone_abbreviation")
            .and_then(|l| l.as_str())
            .ok_or(eyre::eyre!("Timezone abbreviation not found"))?
            .to_string();

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
                                .map(|t| t.as_str().map(|t| t.to_string()))
                                .collect_vec();

                            match timestamps.iter().any(|t| t.is_none()) {
                                true => Err(eyre::eyre!("Couldn't parse timestamps")),
                                false => Ok(timestamps
                                    .into_iter()
                                    .map(|t| t.unwrap().to_string())
                                    .collect_vec()),
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
                                    false => Ok(temperatures
                                        .into_iter()
                                        .map(|t| t.unwrap())
                                        .collect_vec()),
                                }
                            }
                            _ => Err(eyre::eyre!("Couldn't parse temperatures")),
                        }
                    }?;

                    Ok((timestamps, temperatures))
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
}

#[derive(Debug)]
struct CurrentWeatherData {
    time: String,
    temperature: f64,
    weather_code: WeatherCode,
    wind_speed: f64,
    wind_direction: WindDirection,
}

impl CurrentWeatherData {
    fn from_json(json: &Map<String, Value>) -> eyre::Result<Self> {
        let time = json
            .get("time")
            .and_then(|t| t.as_str().map(|t| t.to_string()))
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
            wind_direction,
        })
    }
}

#[derive(Default, Debug)]
enum WeatherCode {
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

#[derive(Default, Debug, Sequence)]
enum WindDirection {
    #[default]
    Unknown,
    N,
    NNE,
    NE,
    ENE,
    E,
    ESE,
    SE,
    SSE,
    S,
    SSW,
    SW,
    WSW,
    W,
    WNW,
    NW,
    NNW,
}

impl Display for WindDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            WindDirection::Unknown => write!(f, "Unknown"),
            WindDirection::N => write!(f, "N"),
            WindDirection::NNE => write!(f, "NNE"),
            WindDirection::NE => write!(f, "NE"),
            WindDirection::ENE => write!(f, "ENE"),
            WindDirection::E => write!(f, "E"),
            WindDirection::ESE => write!(f, "ESE"),
            WindDirection::SE => write!(f, "SE"),
            WindDirection::SSE => write!(f, "SSE"),
            WindDirection::S => write!(f, "S"),
            WindDirection::SSW => write!(f, "SSW"),
            WindDirection::SW => write!(f, "SW"),
            WindDirection::WSW => write!(f, "WSW"),
            WindDirection::W => write!(f, "W"),
            WindDirection::WNW => write!(f, "WNW"),
            WindDirection::NW => write!(f, "NW"),
            WindDirection::NNW => write!(f, "NNW"),
        }
    }
}

type DegreeRanges = (Option<(f64, f64)>, Option<(f64, f64)>);

impl WindDirection {
    /// http://snowfence.umn.edu/Components/winddirectionanddegrees.htm
    fn degree_ranges(&self) -> DegreeRanges {
        match self {
            WindDirection::Unknown => (None, None),
            WindDirection::N => (Some((0.0, 11.25)), Some((348.75, 360.0))),
            _ => (
                Some(match self {
                    WindDirection::NNE => (11.25, 33.75),
                    WindDirection::NE => (33.75, 56.25),
                    WindDirection::ENE => (56.25, 78.75),
                    WindDirection::E => (78.75, 101.25),
                    WindDirection::ESE => (101.25, 123.75),
                    WindDirection::SE => (123.75, 146.25),
                    WindDirection::SSE => (146.25, 168.75),
                    WindDirection::S => (168.75, 191.25),
                    WindDirection::SSW => (191.25, 213.75),
                    WindDirection::SW => (213.75, 236.25),
                    WindDirection::WSW => (236.25, 258.75),
                    WindDirection::W => (258.75, 281.25),
                    WindDirection::WNW => (281.25, 303.75),
                    WindDirection::NW => (303.75, 326.25),
                    WindDirection::NNW => (326.25, 348.75),
                    _ => unreachable!(),
                }),
                None,
            ),
        }
    }

    fn from_degrees(degrees: f64) -> Self {
        let deg = (degrees % 360.0).round();

        enum_iterator::all::<Self>()
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
            .unwrap()
    }
}
