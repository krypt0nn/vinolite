use std::io::Stdout;

use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::crossterm::event::{self, Event, KeyCode};

use super::db_stats::Table;

fn format_bytes(mut bytes: f64) -> String {
    for suffix in ["B", "KB", "MB", "GB"] {
        // This is intended, e.g. to have `0.98 KB` instead of `1000 B`.
        if bytes < 1000.0 {
            return format!("{bytes:.2} {suffix}");
        }

        bytes /= 1024.0;
    }

    format!("{bytes:.2} TB")
}

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
        (table.size + table.indexes.iter().map(|index| index.size).sum::<u64>()) as f64
    }

    let total_tables_size = tables.iter().map(table_size).sum::<f64>();

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
                Span::from("Q").fg(Color::Red), Span::from("uit "),
                Span::from("←→").fg(Color::Red), Span::from(" Select table "),
                Span::from("↑↓").fg(Color::Red), Span::from(" Table details "),
                Span::from("Enter").fg(Color::Red), Span::from(" Switch page ")
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

                        let real_table_fraction = table_size / total_tables_size;
                        let norm_table_fraction = table_size.log2() / total_tables_size.log2();

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
                            Constraint::Ratio((table.size as f64 / table_size * u32::MAX as f64) as u32, u32::MAX)
                        ]).areas(inner_bar_area);

                        let index_size_bar = Block::new().bg(Color::Yellow);
                        let table_size_bar = Block::new().bg(Color::Blue);

                        frame.render_widget(index_size_bar, index_bar_area);
                        frame.render_widget(table_size_bar, table_bar_area);
                    }

                    let table_fraction = table_size(view.table()) / total_tables_size;

                    let bottom_widget = Paragraph::new(Text::from_iter([
                        format!("Table size  : {} ({:.2}% of total)", format_bytes(view.table().size as f64), table_fraction * 100.0),
                        format!("Indexes size: {}", format_bytes(view.table().indexes.iter().map(|index| index.size as f64).sum::<f64>())),
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

                    let table_details_area = table_borders_widget.inner(area);

                    frame.render_widget(table_borders_widget, area);

                    let total_columns_size = view.table().columns.iter()
                        .map(|column| column.length as f64)
                        .sum::<f64>();

                    let (table_columns, sizes) = view.table().columns.iter()
                        .map(|column| {
                            let norm_column_fraction = (column.length as f64).log2() / total_columns_size.log2();

                            let name = column.name.as_str();
                            let format = column.format.to_string();
                            let size = format_bytes(column.length as f64);
                            let fraction = format!("{:.2}%", column.length as f64 / total_columns_size * 100.0);

                            let sizes = (name.len(), format.len(), size.len(), fraction.len());

                            let row = (
                                Line::from(name),
                                Line::from(format),
                                Line::from(size),
                                Line::from(fraction),
                                norm_column_fraction
                            );

                            (row, sizes)
                        })
                        .collect::<(Vec<_>, Vec<_>)>();

                    let sizes = sizes.into_iter().fold((4, 4, 9, 8), |acc, sizes| (
                        acc.0.max(sizes.0),
                        acc.1.max(sizes.1),
                        acc.2.max(sizes.2),
                        acc.3.max(sizes.2)
                    ));

                    let [table_columns_area, area] = Layout::vertical([
                        Constraint::Length(view.table().columns.len() as u16 + 3),
                        Constraint::Fill(1)
                    ]).areas(table_details_area);

                    let table_columns_block_widget = Block::bordered().title_top("Columns");

                    let table_columns_inner_area = table_columns_block_widget.inner(table_columns_area);

                    frame.render_widget(Block::bordered().title_top("Columns"), table_columns_area);

                    let [table_columns_row_area, mut table_columns_inner_area] = Layout::vertical([
                        Constraint::Length(1),
                        Constraint::Fill(1)
                    ]).areas(table_columns_inner_area);

                    let [name_area, type_area, size_area, fraction_area, bar_area] = Layout::horizontal([
                        Constraint::Length(sizes.0 as u16 + 2),
                        Constraint::Length(sizes.1 as u16 + 2),
                        Constraint::Length(sizes.2 as u16 + 2),
                        Constraint::Length(sizes.3 as u16 + 2),
                        Constraint::Fill(1)
                    ]).areas(table_columns_row_area);

                    frame.render_widget(Span::from("Name").underlined(), name_area);
                    frame.render_widget(Span::from("Type").underlined(), type_area);
                    frame.render_widget(Span::from("Disk size").underlined(), size_area);
                    frame.render_widget(Span::from("Fraction").underlined(), fraction_area);
                    frame.render_widget(Span::from("Bar").underlined(), bar_area);

                    for (name_widget, type_widget, size_widget, fraction_widget, norm_column_fraction) in table_columns {
                        let [table_columns_row_area, remaining_table_columns_inner_area] = Layout::vertical([
                            Constraint::Length(1),
                            Constraint::Fill(1)
                        ]).areas(table_columns_inner_area);

                        table_columns_inner_area = remaining_table_columns_inner_area;

                        let [name_area, type_area, size_area, fraction_area, bar_area] = Layout::horizontal([
                            Constraint::Length(sizes.0 as u16 + 2),
                            Constraint::Length(sizes.1 as u16 + 2),
                            Constraint::Length(sizes.2 as u16 + 2),
                            Constraint::Length(sizes.3 as u16 + 2),
                            Constraint::Fill(1)
                        ]).areas(table_columns_row_area);

                        frame.render_widget(name_widget, name_area);
                        frame.render_widget(type_widget, type_area);
                        frame.render_widget(size_widget, size_area);
                        frame.render_widget(fraction_widget, fraction_area);

                        let [bar_area, _] = Layout::horizontal([
                            Constraint::Ratio((norm_column_fraction * u32::MAX as f64) as u32, u32::MAX),
                            Constraint::Fill(1)
                        ]).areas(bar_area);

                        frame.render_widget(Block::new().bg(Color::Blue), bar_area);
                    }
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

                        KeyCode::Enter => {
                            view.page = match view.page {
                                Page::TablesChart  => Page::TableDetails,
                                Page::TableDetails => Page::TablesChart
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
