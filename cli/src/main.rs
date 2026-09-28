//! bosun — machine and shell bootstrap.

use std::process::ExitCode;

use clap::Parser;

use bosun::app::App;
use bosun::cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();

    let mut app = match App::new(&cli.global) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("bosun: {e:#}");
            return ExitCode::FAILURE;
        }
    };

    match bosun::commands::dispatch(&mut app, &cli.command) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(e) => {
            // `{:#}` prints the whole anyhow context chain, which is where the
            // "run this instead" hints live.
            eprintln!("bosun: {e:#}");
            ExitCode::FAILURE
        }
    }
}
