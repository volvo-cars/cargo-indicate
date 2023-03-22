#![forbid(unsafe_code)]
#![feature(path_file_prefix)]
use std::{
    cell::RefCell, collections::BTreeSet, ffi::OsString, fs, path::PathBuf,
    rc::Rc,
};

use clap::{ArgGroup, CommandFactory, Parser};
use indicate::{
    advisory::AdvisoryClient, execute_query_with_adapter, query::FullQuery,
    query::FullQueryBuilder, repo::github::GitHubClient,
    util::transparent_results, CargoOpt, IndicateAdapterBuilder, ManifestPath,
};

/// Program to query Rust dependencies
#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
#[command(group(
    ArgGroup::new("query_inputs")
        .multiple(true) // We can have `--query-dir` AND `--query-path`
        .required(true)
))]
struct IndicateCli {
    /// Indicate queries in a supported file format to be run in series
    ///
    /// These queries will run using the same Trustfall adapter, meaning there
    /// is a performance gain versus multiple separate `cargo-indicate` calls.
    #[arg(
        short = 'Q',
        long,
        group = "query_inputs",
        num_args = 1..,
        value_name = "FILE",
        value_hint = clap::ValueHint::FilePath
    )]
    query_path: Option<Vec<PathBuf>>,

    /// A directory containing indicate queries in a supported file format
    ///
    /// Will create file names depending on the names of the input query files;
    /// if there are duplicate query names, a number will be appended to avoid
    /// overwriting. The extension will be `.out.json`.
    ///
    /// These queries will run using the same Trustfall adapter, meaning there
    /// is a performance gain versus multiple separate `cargo-indicate` calls.
    #[arg(
        short = 'd',
        long,
        group = "query_inputs",
        value_name = "DIR",
        value_hint = clap::ValueHint::DirPath
    )]
    query_dir: Option<PathBuf>,

    /// Exclude files containing this substring when using `--query-dir`
    #[arg(short = 'x', num_args = 0.., long, requires = "query_dir")]
    exclude: Vec<String>,

    /// Indicate queries in plain text, without arguments, to be run in series
    ///
    /// These queries will run using the same Trustfall adapter, meaning there
    /// is a performance gain versus multiple separate `cargo-indicate` calls.
    #[arg(
        short, long,
        num_args = 1..,
        group = "query_inputs", 
        conflicts_with_all = ["query_path", "query_dir"]
    )]
    query: Option<Vec<String>>,

    /// Indicate arguments including arguments in plain text, without query in a
    /// JSON format
    ///
    /// If more than one query was provided, the args will be mapped to the
    /// queries in the same order. If the number of args _n_ is less than the number
    /// of queries, empty args will be used for all queries _m > n_.
    #[arg(short, long, num_args = 0.., requires = "query_inputs")]
    args: Option<Vec<String>>,

    /// Path to a Cargo.toml file, or a directory containing one
    #[arg(
        last(true),
        required_unless_present = "show_schema",
        default_value = "./",
        value_hint = clap::ValueHint::AnyPath
    )]
    package: PathBuf,

    /// Specify the package name that is to be parsed from the package path, if
    /// it might be a workspace
    ///
    /// Use this if the target directory might be a workspace. If it is certain,
    /// point directly to the package in the `package` parameter instead
    #[arg(long)]
    package_name: Option<String>,

    /// Define another output than stdout for query results
    ///
    /// If more than one is provided, it must be the same number as the number
    /// of queries provided, and query _i_ will be located in the _i_ defined
    /// output.
    #[arg(
        short,
        long,
        num_args = 1..,
        value_name = "FILE",
        value_hint = clap::ValueHint::FilePath
    )]
    output: Option<Vec<PathBuf>>,

    /// Define a directory to write query results to, recursively creating
    /// directories if needed
    ///
    /// The results will be placed in files in accordance with their filename
    /// with the extension replaced with `.out.json`.
    #[arg(
        short = 'O',
        long,
        value_name = "DIR",
        value_hint = clap::ValueHint::DirPath,
        conflicts_with = "output",
        conflicts_with = "query"
    )]
    output_dir: Option<PathBuf>,

    /// The max number of query results to evaluate,
    /// use to limit for example third party API calls
    #[arg(short = 'm', long, value_name = "INTEGER")]
    max_results: Option<usize>,

    /// Outputs the schema that is used to write queries,
    /// in a GraphQL format, and exits
    #[arg(
        long,
        exclusive = true,
        // Hack due to clap not supporting `required_unless` for groups
        group = "query_inputs"
    )]
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
    #[arg(short, long, num_args=0.., conflicts_with = "all_features")]
    features: Option<Vec<String>>,

    /// Use a local `advisory-db` database instead of fetching the default
    /// from GitHub
    #[arg(long, value_hint = clap::ValueHint::DirPath)]
    advisory_db_dir: Option<PathBuf>,

    /// Attempt to use a cached version of `advisory-db` from the default
    /// location; Will fetch a new one if not present
    #[arg(long, conflicts_with = "advisory_db_dir")]
    cached_advisory_db: bool,

    /// If the program should sleep while awaiting a new GitHub API quota, if it
    /// is reached during execution
    ///
    /// This can sleep for a loong time, so only recommended use is in automated
    /// invocations where execution time is not important.
    #[arg(long)]
    await_github_quota: bool,
}

fn main() {
    let cli = IndicateCli::parse();

    // Used to report errors
    let mut cmd = IndicateCli::command();

    if cli.show_schema {
        println!("{}", indicate::RAW_SCHEMA);
        return;
    }

    // Aggregate query paths from `--query-path` and `--query-dir` flags
    let query_paths: Option<Vec<PathBuf>> = if cli.query_path.is_some()
        || cli.query_dir.is_some()
    {
        let mut q = Vec::new();

        if let Some(query_paths) = cli.query_path {
            q.extend(query_paths);
        }

        if let Some(dir_path) = cli.query_dir {
            let files = fs::read_dir(&dir_path).unwrap_or_else(|e| {
                cmd.error(
                    clap::error::ErrorKind::InvalidValue,
                format!(
                            "could not read queries in directory {} due to error: {e}",
                            dir_path.to_string_lossy()
                            ),
                    )
                    .exit();
            });

            for f in files {
                let file_path = f
                    .unwrap_or_else(|e| {
                        panic!(
                            "could not read file in {} due to error {e}",
                            dir_path.to_string_lossy()
                        )
                    })
                    .path();

                if file_path.is_dir() {
                    let msg = format!(
                        "nested directories with --query-dir not supported, found {}",
                        file_path.to_string_lossy()
                    );
                    cmd.error(clap::error::ErrorKind::ValueValidation, msg)
                        .exit();
                } else if cli.exclude.contains(
                    &file_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                ) {
                    // Don't add this, it is included in list of excluded files
                    continue;
                } else {
                    q.push(file_path);
                }
            }
        }

        Some(q)
    } else {
        None
    };

    let mut fqs: Vec<FullQuery>;
    if let Some(query_paths) = &query_paths {
        fqs = Vec::with_capacity(query_paths.len());
        for path in query_paths {
            fqs.push(FullQuery::from_path(path).unwrap_or_else(|e| {
                panic!(
                    "could not parse query file {} due to error: {e}",
                    path.to_string_lossy()
                );
            }));
        }
    } else if let Some(query_strs) = cli.query {
        if let Some(args) = &cli.args {
            if args.len() > query_strs.len() {
                cmd.error(
                    clap::error::ErrorKind::TooManyValues,
                    "more arguments provided than queries",
                )
                .exit();
            }
        }

        fqs = Vec::with_capacity(query_strs.len());
        let mut args = cli.args.iter().flatten();

        // Queries with index over the amount of arguments get no arguments
        for query_str in query_strs {
            let mut fqb = FullQueryBuilder::new(query_str);

            if let Some(args) = args.next() {
                fqb =
                    fqb.args(serde_json::from_str(args).unwrap_or_else(|e| {
                        let msg = format!(
                            "could not parse args argument due to error: {e}"
                        );
                        cmd.error(clap::error::ErrorKind::ValueValidation, msg)
                            .exit();
                    }));
            }

            fqs.push(fqb.build());
        }
    } else {
        unreachable!("no query provided");
    }

    // If empty directory was provided we check that here
    if fqs.is_empty() {
        cmd.error(clap::error::ErrorKind::TooFewValues, "no queries provided")
            .exit();
    }

    // Test this early, so we panic before anything expensive is done
    if let Some(output_paths) = &cli.output {
        // If we have more than one output, it must be a list of files to write
        // each query to
        if output_paths.len() > 1 && output_paths.len() != fqs.len() {
            cmd
                .error(
                    clap::error::ErrorKind::WrongNumberOfValues,
                    "if more than one output path is defined, it must match the amount of queries"
                )
                .exit();
        }
    }

    let manifest_path = if let Some(package_name) = cli.package_name {
        ManifestPath::with_package_name(cli.package, package_name)
    } else {
        ManifestPath::new(cli.package)
    };

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

    if cli.await_github_quota {
        b = b.github_client(GitHubClient::new(true));
    }

    // Reuse the same adapter for multiple queries
    let adapter = Rc::new(RefCell::new(b.build()));

    let mut res_strings = Vec::with_capacity(fqs.len());
    for query in fqs {
        let res = execute_query_with_adapter(
            &query,
            Rc::clone(&adapter),
            cli.max_results,
        );
        let transparent_res = transparent_results(res);
        res_strings.push(
            serde_json::to_string_pretty(&transparent_res)
                .expect("could not serialize result"),
        );
    }

    // Use provided outputs, or create them in a directory, bases on the query
    // file names. `cli.output` and `cli.output_dir` are exclusive, guaranteed
    // by clap
    let output_paths: Option<Vec<PathBuf>> = if let Some(paths) = cli.output {
        // Assertion for amount of queries - amount of output paths done before
        Some(paths)
    } else if let Some(dir_path) = cli.output_dir {
        // Ensure we have a proper directory to write to
        let dir_root = if dir_path.is_dir() {
            dir_path
        } else if dir_path.exists() && !dir_path.is_dir() {
            cmd.error(
                clap::error::ErrorKind::ValueValidation,
                "provided output path is not a directory",
            )
            .exit();
        } else {
            // It does not exist, so we try to create it (recursively)
            fs::create_dir_all(&dir_path).unwrap_or_else(|e| {
                panic!("could not create output dir (recursively) due to error: {e}")
            });
            dir_path
        };

        // We generate the file names from the names of our input queries
        // unwrap is safe, since clap ensures --output-dir cannot be used
        // with non-file queries
        let mut used_file_prefix: BTreeSet<OsString> = BTreeSet::new();
        Some(
            query_paths
                .unwrap()
                .iter()
                .map(|p| {
                    let mut pb = PathBuf::from(&dir_root);

                    let true_file_prefix = match p.file_prefix() {
                        Some(fs) => fs,
                        None => panic!(
                            "could not extract file stem from {}",
                            p.to_string_lossy()
                        ),
                    };

                    // To avoid overwriting when we have duplicate query name stems,
                    // we append a number to the stem when they are duplicates.
                    let file_prefix = if !used_file_prefix
                        .contains(true_file_prefix)
                    {
                        used_file_prefix
                            .insert(true_file_prefix.to_os_string());
                        OsString::from(true_file_prefix)
                    } else {
                        let mut i: u32 = 1;
                        let mut file_prefix = OsString::from(true_file_prefix);
                        while used_file_prefix.contains(file_prefix.as_os_str())
                        {
                            // This is to avoid file_prefix-1-2-3-4-....
                            file_prefix = OsString::from(true_file_prefix);
                            file_prefix.push(i.to_string());
                            i += 1;
                        }
                        used_file_prefix.insert(file_prefix.clone());
                        file_prefix
                    };

                    pb.push(file_prefix);
                    pb.set_extension("out.json"); // first  `.` inserted automatically

                    pb
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };

    // At this point we have already checked that the amount of outputs is acceptable
    // in accordance with how many queries there are
    if let Some(output_paths) = output_paths {
        match output_paths {
            single_path if output_paths.len() == 1 => {
                // Write all queries to a single file
                let concat_res = res_strings.join("\n");

                fs::write(
                    single_path[0].as_path(),
                    concat_res
                ).unwrap_or_else(|e| {
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
                            path.to_string_lossy())
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
