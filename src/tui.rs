use std::io::Stdout;
use std::sync::Arc;

use spin::Mutex;

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

fn table_size(table: &Table) -> f64 {
    (table.size + table.indexes.iter().map(|index| index.size).sum::<u64>()) as f64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    TablesChart,
    TableDetails,
    VacuumQuestion,
    VacuumProgress
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct View {
    pub page: Page,
    pub tables: Vec<Table>,
    pub selected_table: usize
}

impl View {
    #[inline]
    pub fn table(&self) -> &Table {
        &self.tables[self.selected_table]
    }
}

pub fn run(mut terminal: Terminal<CrosstermBackend<Stdout>>, database: rusqlite::Connection) -> anyhow::Result<()> {
    let view = Arc::new(Mutex::new(View {
        page: Page::TablesChart,
        tables: super::db_stats::query_structure(&database)?,
        selected_table: 0
    }));

    let total_tables_size = view.lock().tables.iter().map(table_size).sum::<f64>();

    loop {
        let view_copy = view.clone();

        terminal.draw(move |frame| {
            let view = view_copy.lock();

            let [area, footer_area] = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(1)
            ]).areas(frame.area());

            frame.render_widget(Line::from_iter([
                Span::from("Q").red(), Span::from("uit "),
                Span::from("V").red(), Span::from("acuum "),
                Span::from("←→").red(), Span::from(" Select table "),
                Span::from("↑↓").red(), Span::from(" Table details "),
                Span::from("Enter").red(), Span::from(" Switch page ")
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

                        let Some(table) = view.tables.get(i) else {
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

                        let index_size_bar = Block::new().on_yellow();
                        let table_size_bar = Block::new().on_blue();

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

                    // ===================== Columns table =====================

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

                        frame.render_widget(Block::new().on_blue(), bar_area);
                    }

                    // ===================== Indexes table =====================

                    let total_indexes_size = view.table().indexes.iter()
                        .map(|index| index.size as f64)
                        .sum::<f64>();

                    let (table_indexes, sizes) = view.table().indexes.iter()
                        .map(|index| {
                            let norm_index_fraction = (index.size as f64).log2() / total_indexes_size.log2();

                            let name = index.name.as_str();
                            let size = format_bytes(index.size as f64);
                            let fraction = format!("{:.2}%", index.size as f64 / total_indexes_size * 100.0);

                            let sizes = (name.len(), size.len(), fraction.len());

                            let row = (
                                Line::from(name),
                                Line::from(size),
                                Line::from(fraction),
                                norm_index_fraction
                            );

                            (row, sizes)
                        })
                        .collect::<(Vec<_>, Vec<_>)>();

                    let sizes = sizes.into_iter().fold((4, 9, 8), |acc, sizes| (
                        acc.0.max(sizes.0),
                        acc.1.max(sizes.1),
                        acc.2.max(sizes.2)
                    ));

                    let [table_indexes_area, _] = Layout::vertical([
                        Constraint::Length(view.table().indexes.len() as u16 + 3),
                        Constraint::Fill(1)
                    ]).areas(area);

                    let table_indexes_block_widget = Block::bordered().title_top("Columns");

                    let table_indexes_inner_area = table_indexes_block_widget.inner(table_indexes_area);

                    frame.render_widget(Block::bordered().title_top("Indexes"), table_indexes_area);

                    let [table_indexes_row_area, mut table_indexes_inner_area] = Layout::vertical([
                        Constraint::Length(1),
                        Constraint::Fill(1)
                    ]).areas(table_indexes_inner_area);

                    let [name_area, size_area, fraction_area, bar_area] = Layout::horizontal([
                        Constraint::Length(sizes.0 as u16 + 2),
                        Constraint::Length(sizes.1 as u16 + 2),
                        Constraint::Length(sizes.2 as u16 + 2),
                        Constraint::Fill(1)
                    ]).areas(table_indexes_row_area);

                    frame.render_widget(Span::from("Name").underlined(), name_area);
                    frame.render_widget(Span::from("Disk size").underlined(), size_area);
                    frame.render_widget(Span::from("Fraction").underlined(), fraction_area);
                    frame.render_widget(Span::from("Bar").underlined(), bar_area);

                    for (name_widget, size_widget, fraction_widget, norm_index_fraction) in table_indexes {
                        let [table_indexes_row_area, remaining_table_indexes_inner_area] = Layout::vertical([
                            Constraint::Length(1),
                            Constraint::Fill(1)
                        ]).areas(table_indexes_inner_area);

                        table_indexes_inner_area = remaining_table_indexes_inner_area;

                        let [name_area, size_area, fraction_area, bar_area] = Layout::horizontal([
                            Constraint::Length(sizes.0 as u16 + 2),
                            Constraint::Length(sizes.1 as u16 + 2),
                            Constraint::Length(sizes.2 as u16 + 2),
                            Constraint::Fill(1)
                        ]).areas(table_indexes_row_area);

                        frame.render_widget(name_widget, name_area);
                        frame.render_widget(size_widget, size_area);
                        frame.render_widget(fraction_widget, fraction_area);

                        let [bar_area, _] = Layout::horizontal([
                            Constraint::Ratio((norm_index_fraction * u32::MAX as f64) as u32, u32::MAX),
                            Constraint::Fill(1)
                        ]).areas(bar_area);

                        frame.render_widget(Block::new().on_yellow(), bar_area);
                    }
                }

                Page::VacuumQuestion => {
                    let [_, message_area, _] = Layout::vertical([
                        Constraint::Fill(1),
                        Constraint::Length(11),
                        Constraint::Fill(1)
                    ]).areas(area);

                    frame.render_widget(Block::new().on_yellow(), message_area);

                    let [_, message_area, _] = Layout::horizontal([
                        Constraint::Fill(1),
                        Constraint::Length(40),
                        Constraint::Fill(1)
                    ]).areas(message_area);

                    frame.render_widget(Text::from_iter([
                        Line::from(""),
                        Line::from("Vacuum database").bold(),
                        Line::from(""),
                        Line::from("Rebuild the database file, repacking it"),
                        Line::from("into a minimal amount of disk space."),
                        Line::from(""),
                        Line::from("This operation can take some time."),
                        Line::from("Make a backup prior that."),
                        Line::from(""),
                        Line::from("Press enter to continue.").bold(),
                        Line::from("")
                    ]), message_area);
                }

                Page::VacuumProgress => {
                    let [_, message_area, _] = Layout::vertical([
                        Constraint::Fill(1),
                        Constraint::Length(5),
                        Constraint::Fill(1)
                    ]).areas(area);

                    frame.render_widget(Block::new().on_yellow(), message_area);

                    let [_, message_area, _] = Layout::horizontal([
                        Constraint::Fill(1),
                        Constraint::Length(40),
                        Constraint::Fill(1)
                    ]).areas(message_area);

                    frame.render_widget(Text::from_iter([
                        Line::from(""),
                        Line::from("Database rebuilding is in progress").bold(),
                        Line::from(""),
                        Line::from("This operation may take some time."),
                        Line::from("")
                    ]), message_area);
                }
            }
        })?;

        loop {
            let mut view = view.lock();

            if view.page == Page::VacuumProgress {
                database.execute("VACUUM", [])?;

                view.page = Page::TablesChart;
                view.tables = super::db_stats::query_structure(&database)?;

                break;
            }

            if event::poll(std::time::Duration::from_secs(1))? {
                #[allow(clippy::single_match)]
                match event::read()? {
                    Event::Key(key) => match key.code {
                        KeyCode::Char('q') if view.page == Page::VacuumQuestion => view.page = Page::TablesChart,

                        KeyCode::Char('q') => return Ok(()),

                        KeyCode::Char('v') => view.page = Page::VacuumQuestion,

                        KeyCode::Enter if view.page == Page::VacuumQuestion => view.page = Page::VacuumProgress,

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
                            match view.page {
                                Page::TablesChart  => view.page = Page::TableDetails,
                                Page::TableDetails => view.page = Page::TablesChart,

                                _ => ()
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
