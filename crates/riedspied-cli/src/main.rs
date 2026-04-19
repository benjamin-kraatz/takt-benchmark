mod commands;
mod output;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::bench::{BenchmarkChoice, ProfileChoice, list_targets, load_history, run_benchmark};
use output::{print_device_table, print_history, print_run_summary};

#[derive(Debug, Parser)]
#[command(
    name = "riedspied",
    about = "Benchmark mounted devices and storage volumes"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    List,
    Bench {
        #[arg(long)]
        target: String,
        #[arg(long, value_enum, default_value_t = ProfileChoice::Balanced)]
        profile: ProfileChoice,
        #[arg(long = "bench", value_enum)]
        benchmarks: Vec<BenchmarkChoice>,
        #[arg(long)]
        keep_temp_files: bool,
        #[arg(long)]
        no_history: bool,
    },
    History {
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::List => {
            let devices = list_targets()?;
            print_device_table(&devices);
        }
        Command::Bench {
            target,
            profile,
            benchmarks,
            keep_temp_files,
            no_history,
        } => {
            let run = run_benchmark(&target, profile, benchmarks, keep_temp_files, !no_history)?;
            print_run_summary(&run);
        }
        Command::History { limit } => {
            let records = load_history(limit)?;
            print_history(&records);
        }
    }

    Ok(())
}
