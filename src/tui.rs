use std::io::Stdout;

use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::crossterm::event::{self, Event, KeyCode};

use super::db_stats::Table;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    TablesChart,
    TableDetails
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct View<'tables> {
    pub page: Page,
    pub tables: &'tables [Table],
    pub selected_table: usize
}

impl View<'_> {
    #[inline]
    pub const fn table(&self) -> &'_ Table {
        &self.tables[self.selected_table]
    }
}

pub fn run(mut terminal: Terminal<CrosstermBackend<Stdout>>, tables: &[Table]) -> anyhow::Result<()> {
    fn table_size(table: &Table) -> f64 {
        (table.size / 1024 + table.indexes.iter().map(|index| index.size / 1024).sum::<u64>()) as f64
    }

    let total_table_size = tables.iter().map(table_size).sum::<f64>();

    let mut view = View {
        page: Page::TablesChart,
        tables,
        selected_table: 0
    };

    loop {
        terminal.draw(move |frame| {
            let [area, footer_area] = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(1)
            ]).areas(frame.area());

            frame.render_widget(Line::from_iter([
                Span::from("Q").fg(Color::Red), Span::from("uit")
            ]), footer_area);

            match view.page {
                Page::TablesChart => {
                    let [mut top_area, bottom_area] = Layout::vertical([
                        Constraint::Fill(1),
                        Constraint::Length(5)
                    ]).areas(area);

                    let bars_per_page = top_area.width as usize / 6;

                    // TODO shift window following selected_table
                    for i in 0..bars_per_page {
                        let [bar_area, _, remaining_top_area] = Layout::horizontal([
                            Constraint::Length(5),
                            Constraint::Length(1),
                            Constraint::Fill(1)
                        ]).areas(top_area);

                        top_area = remaining_top_area;

                        let Some(table) = tables.get(i) else {
                            break;
                        };

                        let table_size = table_size(table);

                        let real_table_fraction = table_size / total_table_size;
                        let norm_table_fraction = table_size.log2() / total_table_size.log2();

                        let table_ratio = (norm_table_fraction * u32::MAX as f64) as u32;

                        let [_, mut bar_area] = Layout::vertical([
                            Constraint::Fill(1),
                            Constraint::Ratio(table_ratio, u32::MAX)
                        ]).areas(bar_area);

                        if bar_area.height < 4 {
                            bar_area.y -= 4 - bar_area.height;
                            bar_area.height = 4;
                        }

                        let style = if view.selected_table == i {
                            Style::reset().fg(Color::Green)
                        } else {
                            Style::reset()
                        };

                        let bar_widget = Block::bordered()
                            .border_style(style)
                            .title_bottom(format!("{}%", (real_table_fraction * 100.0).round()));

                        let inner_bar_area = bar_widget.inner(bar_area);

                        frame.render_widget(bar_widget, bar_area);

                        let [index_bar_area, table_bar_area] = Layout::vertical([
                            Constraint::Fill(1),
                            Constraint::Ratio(((table.size / 1024) as f64 / table_size * u32::MAX as f64) as u32, u32::MAX)
                        ]).areas(inner_bar_area);

                        let index_size_bar = Block::new().bg(Color::Yellow);
                        let table_size_bar = Block::new().bg(Color::Blue);

                        frame.render_widget(index_size_bar, index_bar_area);
                        frame.render_widget(table_size_bar, table_bar_area);
                    }

                    let bottom_widget = Paragraph::new(Text::from_iter([
                        format!("Table size  : {} bytes", view.table().size),
                        format!("Indexes size: {} bytes", view.table().indexes.iter().map(|index| index.size).sum::<u64>()),
                        format!("Rows        : {}", view.table().rows)
                    ]));

                    let bottom_widget = bottom_widget.block({
                        Block::bordered()
                            .title_top(format!("Table `{}`", view.table().name))
                    });

                    frame.render_widget(bottom_widget, bottom_area);
                }

                Page::TableDetails => {
                    let table_borders_widget = Block::bordered()
                        .title_top(format!("Table `{}`", view.table().name));

                    // let table_details_area = table_borders_widget.inner(area);

                    frame.render_widget(table_borders_widget, area);


                }
            }
        })?;

        loop {
            if event::poll(std::time::Duration::from_secs(1))? {
                #[allow(clippy::single_match)]
                match event::read()? {
                    Event::Key(key) => match key.code {
                        KeyCode::Char('q') => return Ok(()),

                        KeyCode::Left => {
                            #[allow(clippy::implicit_saturating_sub)]
                            if view.selected_table > 0 {
                                view.selected_table -= 1;
                            }
                        }

                        KeyCode::Right => {
                            if view.selected_table + 1 < view.tables.len() {
                                view.selected_table += 1;
                            }
                        }

                        KeyCode::Up => {
                            if view.page == Page::TableDetails {
                                view.page = Page::TablesChart;
                            }
                        }

                        KeyCode::Down => {
                            if view.page == Page::TablesChart {
                                view.page = Page::TableDetails;
                            }
                        }

                        _ => ()
                    }

                    _ => ()
                }

                break;
            }
        }
    }
}
