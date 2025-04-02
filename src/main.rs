use std::path::PathBuf;

pub mod db_stats;
pub mod tui;

fn main() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<String>>();

    if args.len() != 2 {
        eprintln!("Correct command: vinolite <database path>");

        return Ok(());
    }

    let path = PathBuf::from(&args[1]);

    if !path.exists() {
        eprintln!("File {path:?} doesn't exist");

        return Ok(());
    }

    let database = rusqlite::Connection::open(path)?;

    let mut tables = db_stats::query_structure(&database)?;

    tables.sort_by(|a, b| b.size.cmp(&a.size));

    let terminal = ratatui::init();

    tui::run(terminal, &tables)?;

    ratatui::restore();

    Ok(())
}
