use clap::Parser;
use cli::Cli;

use crate::app::App;

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

    let _args = Cli::parse();
    let mut app = App::new()?;
    app.run().await?;
    Ok(())
}
