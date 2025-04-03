use std::path::PathBuf;

pub mod db_stats;
pub mod tui;

const HELP: &str = "
Vinolite  Copyright (C) 2025  Nikita Podvirnyi <krypt0nn@vk.com>
This program comes with ABSOLUTELY NO WARRANTY.
This is free software, and you are welcome to redistribute it
under certain conditions.

Analyze SQLite databases space use per table, column and index.

Usage: vinolite <database path>";

fn main() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<String>>();

    if args.len() != 2 {
        eprintln!("{HELP}");

        return Ok(());
    }

    let path = PathBuf::from(&args[1]);

    if !path.exists() {
        eprintln!("File {path:?} doesn't exist");

        return Ok(());
    }

    let database = rusqlite::Connection::open(path)?;

    let terminal = ratatui::init();

    let result = tui::run(terminal, database);

    ratatui::restore();

    result
}
