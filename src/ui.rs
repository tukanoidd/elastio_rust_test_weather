use std::io::{self, Stdout};

use color_eyre::eyre;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use itertools::Itertools;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{BarChart, Block, BorderType, Borders, Paragraph},
    Frame, Terminal,
};

use crate::{
    data::{CurrentWeatherData, WeatherData},
    providers::ProviderRequestType,
};

pub(crate) fn draw_data(data: WeatherData) -> eyre::Result<()> {
    // Setup terminal
    let mut terminal = setup_terminal_for_drawing()?;

    terminal.draw(|f| draw_weather_data_ui(f, data))?;

    restore_terminal(terminal)
}

fn setup_terminal_for_drawing() -> eyre::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, Clear(ClearType::FromCursorUp))?;
    let backend = CrosstermBackend::new(stdout);

    Ok(Terminal::new(backend)?)
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> eyre::Result<()> {
    // restore terminal
    disable_raw_mode()?;
    //execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
    terminal.show_cursor()?;

    Ok(())
}

fn draw_weather_data_ui(f: &mut Frame<impl Backend>, data: WeatherData) {
    let size = f.size();
    let WeatherData {
        provider,
        request_type,
        requested_date,
        address,
        latitude,
        longitude,
        timezone,
        timezone_abbreviation,
        timestamps,
        temperatures,
        unit,
        current,
    } = data;

    let temp_ts_len = temperatures.len();

    // Outer block
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "Weather in {address} ({longitude}, {latitude}) (Provider: {provider})"
        ))
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Rounded);
    f.render_widget(block, size);

    // The forecast/archive block
    let weather_block_data = timestamps
        .iter()
        .zip(temperatures)
        .map(|(ts, temp)| (ts.as_str(), temp.round() as u64))
        .collect_vec();
    let weather_block = BarChart::default()
        .data(weather_block_data.as_slice())
        .bar_style(Style::default().fg(Color::Cyan))
        .label_style(Style::default().add_modifier(Modifier::ITALIC))
        .value_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    " Weather {} (in {unit}) on {requested_date} [{}] ",
                    match request_type {
                        ProviderRequestType::Forecast => {
                            "Forecast"
                        }
                        ProviderRequestType::History => {
                            "Historical Data"
                        }
                    },
                    match timezone == timezone_abbreviation {
                        true => timezone,
                        false => format!("{timezone} ({timezone_abbreviation})"),
                    }
                ))
                .title_alignment(Alignment::Center)
                .border_type(BorderType::Plain),
        );

    match current {
        Some(CurrentWeatherData {
            time,
            temperature,
            weather_code,
            wind_speed,
            wind_speed_unit,
            wind_direction,
        }) => {
            let horizontal_layout = Layout::default()
                .direction(Direction::Horizontal)
                .margin(2)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                .split(size);

            let current_weather_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain);

            let current_weather_size = &horizontal_layout[0];

            let current_weather_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .vertical_margin(1)
                .horizontal_margin(5)
                .split(*current_weather_size);

            let current_weather_top =
                Paragraph::new(vec![Spans::from("Current Weather"), Spans::from(time)])
                    .alignment(Alignment::Center);

            f.render_widget(current_weather_top, current_weather_layout[0]);

            let current_weather_middle = Paragraph::new(vec![
                Spans::from(format!("Temperature: {temperature} {unit}")),
                Spans::from(weather_code.to_string()),
                Spans::from(""),
                Spans::from(Span::raw(format!(
                    "Wind Speed: {wind_speed} {wind_speed_unit}"
                ))),
                Spans::from(Span::raw(format!("Wind Direction: {wind_direction}"))),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title_alignment(Alignment::Center),
            )
            .alignment(Alignment::Center);

            f.render_widget(current_weather_middle, current_weather_layout[1]);

            f.render_widget(current_weather_block, *current_weather_size);

            let weather_block_size = &horizontal_layout[1];
            f.render_widget(
                weather_block.bar_width(weather_block_size.width / temp_ts_len as u16),
                *weather_block_size,
            );
        }
        None => {
            let layout = Layout::default()
                .margin(2)
                .constraints([Constraint::Percentage(100)])
                .split(size);

            let weather_block_size = &layout[0];

            f.render_widget(
                weather_block.bar_width(weather_block_size.width - 2 / temp_ts_len as u16),
                *weather_block_size,
            )
        }
    }
}
