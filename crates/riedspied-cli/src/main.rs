mod commands;
mod output;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::bench::{
    BenchmarkChoice, ExportFormatChoice, ProfileChoice, export_runs, list_targets, load_history,
    run_benchmark,
};
use output::{print_device_table, print_export_notice, print_history, print_run_summary};

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
    List {
        #[arg(long)]
        verbose: bool,
    },
    Bench {
        #[arg(
            long,
            help = "Target name, mount path, source path, or explicit device ID"
        )]
        target: String,
        #[arg(long, value_enum, default_value_t = ProfileChoice::Balanced)]
        profile: ProfileChoice,
        #[arg(long = "bench", value_enum)]
        benchmarks: Vec<BenchmarkChoice>,
        #[arg(long)]
        keep_temp_files: bool,
        #[arg(long)]
        no_history: bool,
        #[arg(long = "tag")]
        tags: Vec<String>,
        #[arg(long, value_enum)]
        export_format: Option<ExportFormatChoice>,
        #[arg(long)]
        export_path: Option<std::path::PathBuf>,
        #[arg(long)]
        export_title: Option<String>,
    },
    History {
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(
            long,
            help = "Filter by target name, mount path, source path, or explicit device ID"
        )]
        target: Option<String>,
        #[arg(long, value_enum)]
        profile: Option<ProfileChoice>,
        #[arg(long)]
        verbose: bool,
    },
    Export {
        #[arg(long = "run-id")]
        run_ids: Vec<String>,
        #[arg(long)]
        latest: bool,
        #[arg(long, value_enum)]
        format: ExportFormatChoice,
        #[arg(long)]
        output: std::path::PathBuf,
        #[arg(long)]
        title: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::List { verbose } => {
            let devices = list_targets()?;
            print_device_table(&devices, verbose);
        }
        Command::Bench {
            target,
            profile,
            benchmarks,
            keep_temp_files,
            no_history,
            tags,
            export_format,
            export_path,
            export_title,
        } => {
            let run = run_benchmark(
                &target,
                profile,
                benchmarks,
                keep_temp_files,
                !no_history,
                tags,
            )?;
            print_run_summary(&run);
            if let Some(export_format) = export_format {
                let Some(export_path) = export_path else {
                    anyhow::bail!("--export-path is required when --export-format is used");
                };
                riedspied_core::export_runs_to_path(
                    export_format.into(),
                    export_title
                        .as_deref()
                        .unwrap_or("Immediate benchmark export"),
                    std::slice::from_ref(&run),
                    &export_path,
                )?;
                print_export_notice(&format!("{:?}", export_format), &export_path, 1);
            } else if export_path.is_some() {
                anyhow::bail!("--export-format is required when --export-path is used");
            }
        }
        Command::History {
            limit,
            target,
            profile,
            verbose,
        } => {
            let records = load_history(limit, target.as_deref(), profile)?;
            print_history(&records, verbose);
        }
        Command::Export {
            run_ids,
            latest,
            format,
            output,
            title,
        } => {
            let run_count = export_runs(run_ids, latest, format, output.clone(), title)?;
            print_export_notice(&format!("{:?}", format), &output, run_count);
        }
    }

    Ok(())
}
