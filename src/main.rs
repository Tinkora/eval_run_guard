use clap::{Parser, Subcommand};
use eval_run_guard::{audit, load_mapping, render, OutputFormat};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Audit {
        input: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        summary: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "text")]
        format: OutputFormat,
    },
}

fn main() {
    let result = match Cli::parse().command {
        Command::Audit {
            input,
            config,
            summary,
            format,
        } => load_mapping(&config)
            .and_then(|mapping| audit(&input, &mapping, summary.as_deref()))
            .and_then(|report| render(&report, format).map(|output| (report, output))),
    };
    match result {
        Ok((report, output)) => {
            println!("{output}");
            std::process::exit(i32::from(!report.findings.is_empty()));
        }
        Err(error) => {
            eprintln!("eval_run_guard: {error:#}");
            std::process::exit(2);
        }
    }
}
