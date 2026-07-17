#[allow(dead_code)]
mod cli;
#[allow(dead_code)]
mod collect;
#[allow(dead_code)]
mod config;
#[allow(dead_code)]
mod group;
#[allow(dead_code)]
mod model;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::Config {
            action: cli::ConfigAction::Init,
        } => {
            print!("{}", config::Config::init_template());
        }
        cli::Command::Report(_) => println!("report (not yet implemented)"),
        cli::Command::Send { .. } => println!("send (not yet implemented)"),
        cli::Command::Doctor => println!("doctor (not yet implemented)"),
    }
    Ok(())
}
