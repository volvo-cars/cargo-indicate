use std::path::PathBuf;

use clap::Parser;

/// Program to query Rust dependencies
#[derive(Parser, Debug, Clone)]
#[command(author, version, about)]
struct IndicateArgs {
    /// An indicate query path of in a supported file format
    #[arg(short, long)]
    query: PathBuf,

    /// Path to a Cargo.toml file, or a directory containing one
    #[arg(short, long)]
    package: PathBuf,
}

fn main() {
    let args = IndicateArgs::parse();
}
