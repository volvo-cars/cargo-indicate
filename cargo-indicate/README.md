# `cargo-indicate` Run queries against your dependency tree

```console
$ cargo-indicate -h
Program to query Rust dependencies

Usage: cargo-indicate [OPTIONS] [PACKAGE]

Arguments:
  [PACKAGE]  Path to a Cargo.toml file, or a directory containing one [default: ./]

Options:
  -Q, --query-path <FILE>  An indicate query in a supported file format
  -q, --query <QUERY>            An indicate query in plain text, without arguments
  -a, --args <ARGS>              Indicate arguments including arguments in plain text, without query in a JSON format
  -o, --output <FILE>          Define another output than stdout for query results
      --show-schema              Outputs the schema that is used to write queries, in a GraphQL format
  -m, --max-results <INTEGER>                The max number of query results to evaluate, use to limit for example third party API calls
  -h, --help                     Print help
  -V, --version                  Print version

```

You can install the custom command using

```ignore
cargo install --path .
```

which allows you to call the command using `cargo indicate`.

The `indicate` library comes with some test queries and can be used with any
package. For example

```console
$ cargo-indicate -Q ../indicate/test_data/queries/count_dependencies.in.ron
> ../indicate/test_data/fake_crates/direct_dependencies
[
  {
    "dep_name": [],
    "name": "libc",
    "number": 0
  },
  {
    "dep_name": [
      "proc-macro2",
      "unicode-ident",
      "quote",
      "proc-macro2",
      "unicode-ident",
      "unicode-ident"
    ],
    "name": "syn",
    "number": 6
  }
]
```