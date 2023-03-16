## Using both `--output` and `--output-dir` fails

```console
$ cargo-indicate --output test_target/output.out.json --output-dir test_target/will_fail
? failed
error: the argument '--output <FILE>...' cannot be used with '--output-dir <DIR>'

Usage: cargo-indicate --output <FILE>... <--query-path <FILE>...|--query-dir <DIR>|--query <QUERY>...> -- <PACKAGE>

For more information, try '--help'.

```

## Using more `--output` than input queries fails (no `--query-dir`)

```console
$ cargo-indicate
> -Q ../indicate/test_data/queries/direct_dependencies.in.ron
> --output test_target/t1.out.json test_target/t2.out.json
> --
> ../indicate/test_data/fake_crates/simple_deps
? failed
error: if more than one output path is defined, it must match the amount of queries

Usage: cargo-indicate [OPTIONS] <--query-path <FILE>...|--query-dir <DIR>|--query <QUERY>...> -- <PACKAGE>

For more information, try '--help'.

```

## Using more than one `--output`, but not exactly the amount of queries provided fails

_The directory is guaranteed to contain more than two queries_

```console
$ cargo-indicate
> --query-dir ../indicate/test_data/queries/
> --output test_target/t1 test_target/t2
> --
> ../indicate/test_data/fake_crates/simple_deps
? failed
error: if more than one output path is defined, it must match the amount of queries

Usage: cargo-indicate [OPTIONS] <--query-path <FILE>...|--query-dir <DIR>|--query <QUERY>...> -- <PACKAGE>

For more information, try '--help'.

```