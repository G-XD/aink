use clap::Parser;
use cli::Cli;

use crate::app::App;
use crate::components::common::time_filter::TimeFilter;

mod action;
mod app;
mod cli;
mod collector;
mod components;
mod config;
mod errors;
mod exporter;
mod logging;
mod tui;
mod utils;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;

    let args = Cli::parse();
    let initial_filter = match args.range.as_deref() {
        Some(s) => TimeFilter::from_range_arg(s).ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "Invalid --range value '{}'. Expected: today, 7d, 30d, or YYYY-MM-DD..YYYY-MM-DD",
                s
            )
        })?,
        None => TimeFilter::default(),
    };
    let mut app = App::new(initial_filter)?;
    app.run().await?;
    Ok(())
}
