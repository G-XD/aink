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
mod logging;
mod tui;
mod utils;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;

    let args = Cli::parse();
    let initial_filter = args
        .range
        .as_deref()
        .and_then(TimeFilter::from_range_arg)
        .unwrap_or_default();
    let mut app = App::new(initial_filter)?;
    app.run().await?;
    Ok(())
}
