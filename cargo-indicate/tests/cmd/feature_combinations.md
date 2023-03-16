## Using `--all-features` and `--no-default-features` at the same time

```console
$ cargo-indicate --all-features --no-default-features
? failed
error: the argument '--all-features' cannot be used with '--no-default-features'

cargo-indicate --output <FILE>... <--query-path <FILE>...|--query-dir <DIR>|--query <QUERY>...> [-- <PACKAGE>]

For more information, try '--help'.

```

## Using `--all-features` and `--features` at the same time

```console
$ cargo-indicate --all-features --features hello world
? failed
error: the argument '--all-features' cannot be used with '--features <FEATURES>'

cargo-indicate --output <FILE>... <--query-path <FILE>...|--query-dir <DIR>|--query <QUERY>...> [-- <PACKAGE>]

For more information, try '--help'.

```

