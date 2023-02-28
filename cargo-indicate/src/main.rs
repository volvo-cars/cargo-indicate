use std::{fs, path::PathBuf};

use clap::Parser;
use indicate::{
    execute_query, extract_metadata_from_path, query::FullQuery,
    query::FullQueryBuilder, util::transparent_results,
};

/// Program to query Rust dependencies
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct IndicateCli {
    /// An indicate query in a supported file format
    #[arg(short = 'Q', long, group = "query_input")]
    query_path: Option<PathBuf>,

    /// An indicate query in plain text, without arguments
    #[arg(short, long, group = "query_input")]
    query: Option<String>,

    /// Indicate arguments including arguments in plain text, without query in a
    /// JSON format
    #[arg(short, long, requires = "query_input")]
    args: Option<String>,

    /// Path to a Cargo.toml file, or a directory containing one
    #[arg(default_value = "./")]
    package: PathBuf,

    /// Define another output than stdout for query results
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() {
    let cli = IndicateCli::parse();
    let fq: FullQuery;
    if let Some(query_path) = cli.query_path {
        fq = FullQuery::from_path(&query_path).unwrap_or_else(|e| {
            panic!("could not parse query file due to error: {e}");
        });
    } else if let Some(query_str) = cli.query {
        let mut fqb = FullQueryBuilder::new(query_str);

        if let Some(args) = cli.args {
            fqb = fqb.args(
                serde_json::from_str(&args)
                    .expect("could not parse args argument"),
            );
        }

        fq = fqb.build();
    } else {
        unreachable!("no query provided");
    }

    let metadata =
        extract_metadata_from_path(&cli.package).unwrap_or_else(|e| {
            panic!("could not extract metadata due to error: {e}");
        });

    let res = execute_query(&fq, &metadata);
    let transparent_res = transparent_results(res);
    let res_string = serde_json::to_string_pretty(&transparent_res)
        .expect("could not serialize result");
    if let Some(output) = cli.output {
        fs::write(output.as_path(), res_string).unwrap_or_else(|e| {
            panic!(
                "could not write output to {} due to error: {e}",
                output.to_string_lossy()
            )
        });
    } else {
        print!("{res_string}");
    }
}
