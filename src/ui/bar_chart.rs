//! Modified BarChart from tui-rs that allows negative floating point values for the purposes of
//! this project

use unicode_width::UnicodeWidthStr;

use tui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    symbols,
    widgets::{Block, Widget},
};

/// Display multiple bars in a single widgets
///
/// # Examples
///
/// ```
/// # use tui::widgets::{Block, Borders, BarChart};
/// # use tui::style::{Style, Color, Modifier};
/// BarChart::default()
///     .block(Block::default().title("BarChart").borders(Borders::ALL))
///     .bar_width(3)
///     .bar_gap(1)
///     .bar_style(Style::default().fg(Color::Yellow).bg(Color::Red))
///     .value_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
///     .label_style(Style::default().fg(Color::White))
///     .data(&[("B0", 0), ("B1", 2), ("B2", 4), ("B3", 3)])
///     .max(4);
/// ```
#[derive(Debug, Clone)]
pub(crate) struct BarChart<'a> {
    /// Block to wrap the widget in
    block: Option<Block<'a>>,
    /// The width of each bar
    bar_width: u16,
    /// The gap between each bar
    bar_gap: u16,
    /// Set of symbols used to display the data
    bar_set: symbols::bar::Set,
    /// Style of the bars
    bar_style: Style,
    /// Style of the values printed at the bottom of each bar
    value_style: Style,
    /// Style of the labels printed under each bar
    label_style: Style,
    /// Style for the widget
    style: Style,
    /// Slice of (label, value) pair to plot on the chart
    data: &'a [(&'a str, f64)],
    /// Minimum value allowed for the bar chart (since this one can go downwards as well, we might
    /// want to cap off negative values potentially in some cases)
    /// (if the value is not specified, minimum value from the data is taken as reference)
    min: Option<f64>,
    /// Value necessary for a bar to reach the maximum height (if no value is specified,
    /// the maximum value in the data is taken as reference)
    max: Option<f64>,
    /// Values to display on the bar (computed when the data is passed to the widget)
    values: Vec<String>,
}

impl<'a> Default for BarChart<'a> {
    fn default() -> BarChart<'a> {
        BarChart {
            block: None,
            min: None,
            max: None,
            data: &[],
            values: Vec::new(),
            bar_style: Style::default(),
            bar_width: 1,
            bar_gap: 1,
            bar_set: symbols::bar::NINE_LEVELS,
            value_style: Default::default(),
            label_style: Default::default(),
            style: Default::default(),
        }
    }
}

impl<'a> BarChart<'a> {
    pub fn data(mut self, data: &'a [(&'a str, f64)]) -> BarChart<'a> {
        self.data = data;
        self.values = data.iter().map(|(_, v)| v.to_string()).collect();

        self
    }

    pub fn block(mut self, block: Block<'a>) -> BarChart<'a> {
        self.block = Some(block);
        self
    }

    #[allow(dead_code)]
    pub fn min(mut self, min: f64) -> Self {
        self.min = Some(min);
        self
    }

    #[allow(dead_code)]
    pub fn max(mut self, max: f64) -> Self {
        self.max = Some(max);
        self
    }

    pub fn bar_style(mut self, style: Style) -> BarChart<'a> {
        self.bar_style = style;
        self
    }

    pub fn bar_width(mut self, width: u16) -> BarChart<'a> {
        self.bar_width = width;
        self
    }

    #[allow(dead_code)]
    pub fn bar_gap(mut self, gap: u16) -> BarChart<'a> {
        self.bar_gap = gap;
        self
    }

    #[allow(dead_code)]
    pub fn bar_set(mut self, bar_set: symbols::bar::Set) -> BarChart<'a> {
        self.bar_set = bar_set;
        self
    }

    pub fn value_style(mut self, style: Style) -> BarChart<'a> {
        self.value_style = style;
        self
    }

    pub fn label_style(mut self, style: Style) -> BarChart<'a> {
        self.label_style = style;
        self
    }

    #[allow(dead_code)]
    pub fn style(mut self, style: Style) -> BarChart<'a> {
        self.style = style;
        self
    }
}

impl<'a> Widget for BarChart<'a> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        buf.set_style(area, self.style);

        let chart_area = match self.block.take() {
            Some(b) => {
                let inner_area = b.inner(area);
                b.render(area, buf);
                inner_area
            }
            None => area,
        };

        if chart_area.height < 2 {
            return;
        }

        let min =
            self.min
                .unwrap_or(self.data.iter().fold(f64::INFINITY, |min, (_, val)| {
                    match *val < min {
                        true => *val,
                        false => min,
                    }
                }));

        let max =
            self.max
                .unwrap_or(self.data.iter().fold(-f64::INFINITY, |max, (_, val)| {
                    match *val > max {
                        true => *val,
                        false => max,
                    }
                }));
        let max_index = std::cmp::min(
            (chart_area.width / (self.bar_width + self.bar_gap)) as usize,
            self.data.len(),
        );

        let any_negative_values = self.data.iter().take(max_index).any(|(_, v)| *v < 0.0);

        let available_height = match any_negative_values {
            true => chart_area.height / 2,
            false => chart_area.height - 2,
        };

        let mut data = self
            .data
            .iter()
            .take(max_index)
            .map(|&(l, v)| {
                let is_negative = v < 0.0;
                let val = v.abs() as u64 * u64::from(available_height) * 8
                    / std::cmp::max(
                        match is_negative {
                            true => min.abs(),
                            false => max,
                        } as u64,
                        1,
                    );

                (l, val, is_negative && val != 0)
            })
            .collect::<Vec<(&str, u64, bool)>>();

        let zero_line = match any_negative_values {
            true => (chart_area.top() + chart_area.bottom()) / 2,
            false => chart_area.bottom() - 2,
        };

        let symbol = |value| match value {
            0 => self.bar_set.empty,
            1 => self.bar_set.one_eighth,
            2 => self.bar_set.one_quarter,
            3 => self.bar_set.three_eighths,
            4 => self.bar_set.half,
            5 => self.bar_set.five_eighths,
            6 => self.bar_set.three_quarters,
            7 => self.bar_set.seven_eighths,
            _ => self.bar_set.full,
        };

        data.iter_mut()
            .enumerate()
            .for_each(|(i, (_, value, is_negative))| match is_negative {
                true => (0..available_height).for_each(|j| {
                    let symbol = symbol(*value);

                    (0..self.bar_width).for_each(|x| {
                        buf.get_mut(
                            chart_area.left() + i as u16 * (self.bar_width + self.bar_gap) + x,
                            zero_line + j,
                        )
                        .set_symbol(symbol)
                        .set_style(self.bar_style);
                    });

                    *value = value.saturating_sub(8);
                }),
                false => {
                    (0..available_height).for_each(|j| {
                        let symbol = symbol(*value);

                        (0..self.bar_width).for_each(|x| {
                            buf.get_mut(
                                chart_area.left() + i as u16 * (self.bar_width + self.bar_gap) + x,
                                zero_line - j,
                            )
                            .set_symbol(symbol)
                            .set_style(self.bar_style);
                        });

                        *value = value.saturating_sub(8);
                    });
                }
            });

        for (i, &(label, value)) in self.data.iter().take(max_index).enumerate() {
            let val_u64 = value.abs() as u64;
            let is_negative = value < 0.0 && val_u64 != 0;

            if val_u64 != 0 {
                let value_label = &self.values[i];
                let width = value_label.width() as u16;
                if width < self.bar_width {
                    buf.set_string(
                        chart_area.left()
                            + i as u16 * (self.bar_width + self.bar_gap)
                            + (self.bar_width - width) / 2,
                        zero_line,
                        value_label,
                        self.value_style,
                    );
                }
            }

            buf.set_stringn(
                chart_area.left() + i as u16 * (self.bar_width + self.bar_gap),
                match is_negative {
                    true => zero_line - 1,
                    false => zero_line + 1,
                },
                label,
                self.bar_width as usize,
                self.label_style,
            );
        }
    }
}
