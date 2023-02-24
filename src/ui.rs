mod bar_chart;

use std::io::{self, Stdout};

use color_eyre::eyre;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, ScrollUp},
};
use itertools::Itertools;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame, Terminal,
};

use crate::{
    data::{CurrentWeatherData, WeatherData},
    providers::ProviderRequestType,
    ui::bar_chart::BarChart,
};

pub(crate) fn draw_data(data: WeatherData) -> eyre::Result<()> {
    // Setup terminal
    let mut terminal = setup_terminal_for_drawing()?;

    // Draw the frame
    terminal.draw(|f| draw_weather_data_ui(f, data))?;

    // Restore terminal
    restore_terminal(terminal)
}

fn setup_terminal_for_drawing() -> eyre::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // Clear stdout so nothing drawn overlaps with previous text on screen
    execute!(stdout, Clear(ClearType::All))?;
    let backend = CrosstermBackend::new(stdout);

    Ok(Terminal::new(backend)?)
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> eyre::Result<()> {
    // restore terminal
    disable_raw_mode()?;
    // We're scrolling up in case shell prompt decides to overwrite the last line (which happens to me)
    execute!(terminal.backend_mut(), ScrollUp(1))?;
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
        timestamps,
        temperatures,
        unit,
        current,
    } = data;

    // Cache the length of the timestamps and temperatures lists (has to be the same one,
    // and we do the check before this code executes)
    let temp_ts_len = temperatures.len();

    // Outer block
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "Weather in {address} ({latitude}, {longitude}) (Provider: {provider})"
        ))
        .title_alignment(Alignment::Center)
        .border_type(BorderType::Rounded);
    f.render_widget(block, size);

    // The forecast/archive block
    // Setup the data for the bar chart
    let weather_block_data = timestamps
        .iter()
        .zip(temperatures)
        .map(|(ts, temp)| (ts.as_str(), temp))
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
                    " Weather {} (in {unit}) on {requested_date} ",
                    match request_type {
                        ProviderRequestType::Forecast => {
                            "Forecast"
                        }
                        ProviderRequestType::History => {
                            "Historical Data"
                        }
                    }
                ))
                .title_alignment(Alignment::Center)
                .border_type(BorderType::Plain),
        );

    // Check if we have any current weather data
    match current {
        Some(CurrentWeatherData {
            time,
            temperature,
            weather_code,
            wind_speed,
            wind_speed_unit,
            wind_direction,
        }) => {
            // If yes, we set up a horizontal layout, divided into 30%/60% parts to display current
            // weather data and forecast/history data on each side respectively
            let horizontal_layout = Layout::default()
                .direction(Direction::Horizontal)
                .margin(2)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                .split(size);

            // Set up the current weather block
            let current_weather_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain);

            let current_weather_size = &horizontal_layout[0];

            // We divide the current weather block into 30%/70% parts vertical layout
            let current_weather_layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .vertical_margin(1)
                .horizontal_margin(5)
                .split(*current_weather_size);

            // The top part is the "Heading", I put it inside the block because block titles can't
            // be multiline and the string is too long to fit in one line
            let current_weather_heading =
                Paragraph::new(vec![Spans::from("Current Weather"), Spans::from(time)])
                    .alignment(Alignment::Center);

            // Render the "Heading"
            f.render_widget(current_weather_heading, current_weather_layout[0]);

            // The bottom part is the actual data we show
            let current_weather_data = Paragraph::new(vec![
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

            // Render the data
            f.render_widget(current_weather_data, current_weather_layout[1]);

            // Render the current weather block
            f.render_widget(current_weather_block, *current_weather_size);

            // Render the forecast/history block with the chart and set the width of each bar to be
            // evenly distributed across the width of the block
            let weather_block_size = &horizontal_layout[1];
            f.render_widget(
                weather_block.bar_width(weather_block_size.width / temp_ts_len as u16),
                *weather_block_size,
            );
        }
        None => {
            // If we don't have any current weather data, we just render the forecast/history block
            // with a small margin around
            let layout = Layout::default()
                .margin(2)
                .constraints([Constraint::Percentage(100)])
                .split(size);

            let weather_block_size = &layout[0];

            // Render the forecast/history block with the chart and set the width of each bar to be
            // evenly distributed across the width of the block
            f.render_widget(
                weather_block.bar_width(weather_block_size.width / temp_ts_len as u16),
                *weather_block_size,
            )
        }
    }
}
