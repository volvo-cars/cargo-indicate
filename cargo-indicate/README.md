# `cargo-indicate`

This is the cargo add-on for [indicate](./indicate), providing a user-friendly interface to its schema and functionality.

## Installation

To be able to query the unsafety of a package and its dependencies, you need to
install [`cargo-geiger`](https://github.com/rust-secure-code/cargo-geiger).
To do this, simply run:

```ignore
cargo install cargo-geiger
```

Due to a known problem with the `cargo-geiger` `--features` flag, it may not
always work as intended. See 
[the issue on rust-secure-code/cargo-geiger](https://github.com/rust-secure-code/cargo-geiger/issues/379).

You can install the custom command using from source using

```ignore
cargo install --path . --locked
```

in this directory, or from [crates.io](https://crates.io) using

```ignore
cargo install cargo-indicate
```

which allows you to call the command using `cargo indicate`.

Run the following for help

```ignore
cargo indicate --help # or -h
```

The `indicate` library comes with some test queries and can be used with any
package. For example

```console
$ cargo indicate 
> -Q ../indicate/test_data/queries/count_dependencies.in.ron
> --cached-advisory-db # Useful when running concurrent requests, like in tests
> -- ../indicate/test_data/fake_crates/simple_deps
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

## Running Queries

There are currently two ways of running queries, with different pros and cons.
The simlest way is to pass a GraphQL matching the `cargo-indicate` schema (use `--show-schema` to see it),
and then pass eventual arguments in a JSON format. For example,

```console
$ cargo indicate
> --query '{ RootPackage { name @output @filter(op: "=", value: ["$target"]) } }'
> --args '{ "target": "cargo-indicate" }'
> -- .
[
  {
    "name": "cargo-indicate",
  }
]
```

Both the `-q`/`--query` and `-a`/`--args` also take file paths. You can pass
multiple queries and sets of args, and they will be paired.

If you instead want predefined query/arguments pairs, you can use the `-Q`/
`--query-with-args` and `d`/`--query-dir` flags to pass files in a supported
file format (`.ron` files are recommended, see [the test queries](/indicate/
indicate/test_data/queries) for examples).

## Targeting Workspaces

There are two ways to handle workspaces when using `cargo-indicate`:

1. Pass the direct path to a member package directory/`Cargo.toml`-file
2. Use the `--package` flag to specify the name of the package you are
   interested in

The first option is generally preferable, but the second option is useful when
analyzing a lot of packages automatically, and it is unknown if the target dir
is a workspace, but you know the desired package name.

## Selecting sources

Some arguments change the source of data for some signals. For example,
both `--cached-advisory-db` and `--advisory-db-dir` attempts to use a local dir.

Using the local directory (containing no advisories) would succeed, but always
return an empty list

```console
$ cargo indicate
> --advisory-db-dir .
> -Q ../indicate/test_data/queries/advisory_db_simple.in.ron
> -- ../indicate/test_data/fake_crates/known_advisory_deps
[]
```

## Testing

Both `cargo-indicate` and the underlying library `indicate` are tested against
queries and dummy crates. Tests here in `cargo-indicate` ensure the CLI is
working as intended.

It uses [`trycmd`](https://crates.io/crates/trycmd). For more info, see the
[`trycmd` docs](https://docs.rs/trycmd), but the general idea is that tests
compare input and output. Directories `<test-name>.in` are the root of a command
defined in `<test-name>.toml`, and when a `<test-name>.out` directory is present
`trycmd` ensures that after the command in `<test-name>.toml` is run
`<test-name>.in` and `<test-name.out>` is the same (after which they are reset).

This is done using `/tmp` files, so relative files will not work as if actually
being called in `<test-name>.in`.
