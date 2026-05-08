mod cli;
mod commands;
mod config;
mod gh;
mod ide;
mod repo;
mod steps;

use std::process::ExitCode;

use clap::Parser;
use owo_colors::OwoColorize;

use crate::cli::{Cli, Command};

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::New(args) => commands::new::run(args),
        Command::Rm(args) => commands::rm::run(args),
        Command::Clean(args) => commands::clean::run(args),
        Command::Ls(args) => commands::ls::run(args),
        Command::Path(args) => commands::path::run(args),
        Command::Open(args) => commands::open::run(args),
        Command::Prune => commands::prune::run(),
        Command::Completions(args) => commands::completions::run(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{} {:#}", "error:".red().bold(), err);
            ExitCode::FAILURE
        }
    }
}
