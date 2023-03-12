use std::{fs, path::PathBuf};

use clap::{ArgGroup, CommandFactory, Parser};
use indicate::{
    adapter::adapter_builder::IndicateAdapterBuilder, advisory::AdvisoryClient,
    execute_query, execute_query_with_adapter, extract_metadata_from_path,
    query::FullQuery, query::FullQueryBuilder, util::transparent_results,
};

/// Program to query Rust dependencies
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[command(group(
    ArgGroup::new("query_inputs")
        .required(true)
))]
#[command(group(
    ArgGroup::new("adapter_adapters") // Arguments that creates a special IndicateAdapter
        .required(false)
))]
struct IndicateCli {
    /// An indicate query in a supported file format
    #[arg(short = 'Q', long, group = "query_inputs", value_name = "FILE")]
    query_path: Option<PathBuf>,

    /// An indicate query in plain text, without arguments
    #[arg(short, long, group = "query_inputs", conflicts_with = "query_path")]
    query: Option<String>,

    /// Indicate arguments including arguments in plain text, without query in a
    /// JSON format
    #[arg(short, long, requires = "query_inputs")]
    args: Option<String>,

    /// Path to a Cargo.toml file, or a directory containing one
    #[arg(default_value = "./")]
    package: PathBuf,

    /// Define another output than stdout for query results
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// The max number of query results to evaluate,
    /// use to limit for example third party API calls
    #[arg(short = 'm', long, value_name = "INTEGER")]
    max_results: Option<usize>,

    /// Outputs the schema that is used to write queries,
    /// in a GraphQL format
    #[arg(long)]
    show_schema: bool,

    /// Which features to use when resolving metadata for this package
    #[arg(short, long)]
    features: Option<Vec<String>>,

    /// Do not include default features when resolving metadata for this package
    #[arg(short = 'n', long, default_value_t = false)]
    no_default_features: bool,

    /// Use a local `advisory-db` database instead of fetching the default
    /// from GitHub
    #[arg(long, group = "adapter_adapters")]
    advisory_db_dir: Option<PathBuf>,

    /// Attempt to use a cached version of `advisory-db` from the default
    /// location; Will fetch a new one if not present
    #[arg(
        long,
        group = "adapter_adapters",
        conflicts_with = "advisory_db_dir"
    )]
    cached_advisory_db: bool,
}

fn main() {
    let cli = IndicateCli::parse();
    let cmd = IndicateCli::command();

    if cli.show_schema {
        println!("{}", indicate::RAW_SCHEMA);
        return;
    }

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

    let metadata = extract_metadata_from_path(
        &cli.package,
        !cli.no_default_features,
        cli.features,
    )
    .unwrap_or_else(|e| {
        panic!("could not extract metadata due to error: {e}");
    });

    // How we execute the query depends on if the user defined any special
    // requirements for the adapter
    let res = if cmd.get_groups().any(|s| s.get_id() == "adapter_adapters") {
        let mut b = IndicateAdapterBuilder::new(metadata);

        // These two are mutually exclusive, but that is checked by clap already
        if let Some(p) = cli.advisory_db_dir {
            let ac =
                AdvisoryClient::from_path(p.as_path()).unwrap_or_else(|e| {
                    panic!(
                        "could not parse advisory-db in {} due to error: {e}",
                        p.to_string_lossy()
                    )
                });
            b = b.advisory_client(ac);
        } else if cli.cached_advisory_db {
            let ac = AdvisoryClient::from_default_path().unwrap_or_else(|_| {
                AdvisoryClient::new().unwrap_or_else(|e| {
                    panic!("could not fetch advisory-db due to error: {e} (cache also failed)")
                })
            });
            b = b.advisory_client(ac);
        }

        execute_query_with_adapter(&fq, b.build(), cli.max_results)
    } else {
        execute_query(&fq, metadata, cli.max_results)
    };

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
