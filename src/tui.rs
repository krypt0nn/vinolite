use std::io::Stdout;

use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::crossterm::event;

use super::db_stats::Table;

pub fn run(mut terminal: Terminal<CrosstermBackend<Stdout>>, tables: &[Table]) -> anyhow::Result<()> {
    fn table_size(table: &Table) -> f64 {
        (table.size / 1024 + table.indexes.iter().map(|index| index.size / 1024).sum::<u64>()) as f64
    }

    let total_table_size = tables.iter().map(table_size).sum::<f64>();

    let mut selected_table = 0;

    loop {
        terminal.draw(move |frame| {
            let [mut top_area, bottom_area, footer_area] = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(5),
                Constraint::Length(1)
            ]).areas(frame.area());

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

                let style = if selected_table == i {
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
                format!("Table size  : {} bytes", tables[selected_table].size),
                format!("Indexes size: {} bytes", tables[selected_table].indexes.iter().map(|index| index.size).sum::<u64>()),
                format!("Rows        : {}", tables[selected_table].rows)
            ]));

            let bottom_widget = bottom_widget.block({
                Block::bordered()
                    .title_top(format!("Table `{}`", tables[selected_table].name))
            });

            frame.render_widget(bottom_widget, bottom_area);

            frame.render_widget(Line::from_iter([
                Span::from("Q").fg(Color::Red), Span::from("uit")
            ]), footer_area);
        })?;

        loop {
            if event::poll(std::time::Duration::from_secs(1))? {
                #[allow(clippy::single_match)]
                match event::read()? {
                    event::Event::Key(key) => match key.code {
                        event::KeyCode::Char('q') => return Ok(()),

                        event::KeyCode::Left => {
                            #[allow(clippy::implicit_saturating_sub)]
                            if selected_table > 0 {
                                selected_table -= 1;
                            }
                        }

                        event::KeyCode::Right => {
                            if selected_table + 1 < tables.len() {
                                selected_table += 1;
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
