#![forbid(unsafe_code)]
use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};

use clap::{ArgGroup, Parser};
use indicate::{
    advisory::AdvisoryClient, execute_query_with_adapter, query::FullQuery,
    query::FullQueryBuilder, util::transparent_results, CargoOpt,
    IndicateAdapterBuilder, ManifestPath,
};

/// Program to query Rust dependencies
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[command(group(
    ArgGroup::new("query_inputs")
        .required(true)
))]
struct IndicateCli {
    /// Indicate queries in a supported file format
    #[arg(short = 'Q', long, group = "query_inputs", value_name = "FILE")]
    query_path: Option<Vec<PathBuf>>,

    /// Indicate queries in plain text, without arguments
    #[arg(short, long, group = "query_inputs", conflicts_with = "query_path")]
    query: Option<Vec<String>>,

    /// Indicate arguments including arguments in plain text, without query in a
    /// JSON format
    ///
    /// If more than one query was provided, the args will be mapped to the
    /// queries in the same order. If the number of args _n_ is less than the number
    /// of queries, empty args will be used for all queries _m > n_.
    #[arg(short, long, requires = "query_inputs")]
    args: Option<Vec<String>>,

    /// Path to a Cargo.toml file, or a directory containing one
    #[arg(default_value = "./")]
    package: PathBuf,

    /// Define another output than stdout for query results
    ///
    /// If more than one is provided, it must be the same number as the number
    /// of queries provided, and query _i_ will be located in the _i_ defined
    /// output.
    #[arg(short, long, value_name = "FILE")]
    output: Option<Vec<PathBuf>>,

    /// The max number of query results to evaluate,
    /// use to limit for example third party API calls
    #[arg(short = 'm', long, value_name = "INTEGER")]
    max_results: Option<usize>,

    /// Outputs the schema that is used to write queries,
    /// in a GraphQL format
    #[arg(long)]
    show_schema: bool,

    /// Use all available features when resolving metadata for this package
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "no_default_features"
    )]
    all_features: bool,

    /// Do not include default features when resolving metadata for this package
    #[arg(short = 'n', long, default_value_t = false)]
    no_default_features: bool,

    /// Which features to use when resolving metadata for this package
    #[arg(short, long, conflicts_with = "all_features")]
    features: Option<Vec<String>>,

    /// Use a local `advisory-db` database instead of fetching the default
    /// from GitHub
    #[arg(long)]
    advisory_db_dir: Option<PathBuf>,

    /// Attempt to use a cached version of `advisory-db` from the default
    /// location; Will fetch a new one if not present
    #[arg(long, conflicts_with = "advisory_db_dir")]
    cached_advisory_db: bool,
}

fn main() {
    let cli = IndicateCli::parse();

    if cli.show_schema {
        println!("{}", indicate::RAW_SCHEMA);
        return;
    }

    let mut fqs: Vec<FullQuery>;
    if let Some(query_paths) = cli.query_path {
        fqs = Vec::with_capacity(query_paths.len());
        for path in query_paths {
            fqs.push(FullQuery::from_path(&path).unwrap_or_else(|e| {
                panic!("could not parse query file due to error: {e}");
            }));
        }
    } else if let Some(query_strs) = cli.query {
        if let Some(args) = &cli.args {
            if args.len() > query_strs.len() {
                panic!("more arguments provided than queries");
            }
        }

        fqs = Vec::with_capacity(query_strs.len());
        let mut args = cli.args
            .iter()
            .flatten();

        // Queries with index over the amount of arguments get no arguments
        for query_str in query_strs {
            let mut fqb = FullQueryBuilder::new(query_str);

            if let Some(args) = args.next() {
                fqb = fqb.args(
                    serde_json::from_str(&args)
                        .expect("could not parse args argument"),
                );
            }

            fqs.push(fqb.build());
        }
    } else {
        unreachable!("no query provided");
    }

    // Test this early, so we panic before anything expensive is done
    if let Some(output_paths) = &cli.output {
        // If we have more than one output, it must be a list of files to write
        // each query to
        if output_paths.len() > 1 && output_paths.len() != fqs.len() {
            panic!("if more than one output path is defined, it must match the amount of queries");
        }
    }

    let manifest_path = ManifestPath::new(cli.package);

    // How we execute the query depends on if the user defined any special
    // requirements for the adapter

    let mut b = IndicateAdapterBuilder::new(manifest_path);

    // Clap will ensure that these do not mismatch
    if cli.all_features {
        b = b.features(vec![CargoOpt::AllFeatures]);
    } else {
        let mut features = Vec::with_capacity(2);
        if let Some(f) = cli.features {
            features.push(CargoOpt::SomeFeatures(f));
        }
        if cli.no_default_features {
            features.push(CargoOpt::NoDefaultFeatures);
        }
    }

    // These two are mutually exclusive, but that is checked by clap already
    if let Some(p) = cli.advisory_db_dir {
        let ac = AdvisoryClient::from_path(p.as_path()).unwrap_or_else(|e| {
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

    // Reuse the same adapter for multiple queries
    let adapter = Rc::new(RefCell::new(b.build()));

    let mut res_strings = Vec::with_capacity(fqs.len());
    for query in fqs {
        let res = execute_query_with_adapter(&query, Rc::clone(&adapter), cli.max_results);
        let transparent_res = transparent_results(res);
        res_strings.push(
            serde_json::to_string_pretty(&transparent_res)
                .expect("could not serialize result"),
        );
    }

    // At this point we have already checked that the amount of outputs is acceptable
    // in accordance with how many queries there are
    if let Some(output_paths) = cli.output {
        match output_paths {
            single_path if output_paths.len() == 1 => {
                // Write all queries to a single file
                let concat_res = res_strings.join("\n");
                
                fs::write(single_path[0].as_path(), concat_res).unwrap_or_else(|e| {
                    panic!(
                        "could not write output to {} due to error: {e}",
                        single_path[0].to_string_lossy()
                    )
                });
            },
            multiple_paths if output_paths.len() > 1 => {
                // We would have panicked already if these are not equal
                for (res, path) in res_strings.iter().zip(multiple_paths.iter()) {
                    fs::write(path.as_path(), res).unwrap_or_else(|e| {
                        panic!(
                            "could not write output to {} due to error: {e}",
                            path.to_string_lossy()
                        )
                    });
                }
            }
            _ => unreachable!("if more than one output path is defined, it must match the amount of queries"),
        }
    } else {
        let concat_res = res_strings.join("\n");
        print!("{concat_res}");
    }
}
