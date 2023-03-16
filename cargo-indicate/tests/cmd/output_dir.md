## Cannot output into directory when using `-q` as input
```console
$ cargo-indicate -q '' --output-dir test_target/will_fail
> -- ../indicate/test_data/fake_crates/simple_deps
? failed
error: the argument '--query <QUERY>...' cannot be used with '--output-dir <DIR>'

Usage: cargo-indicate <--query-path <FILE>...|--query-dir <DIR>|--query <QUERY>...> -- <PACKAGE>

For more information, try '--help'.

```

## Can output `--query-path` single result in directory
```console
$ cargo-indicate
> --query-path ../indicate/test_data/queries/direct_dependencies.in.ron
> --output-dir test_target
> -- ../indicate/test_data/fake_crates/simple_deps
? success
```

## Can output `--query-path` multiple results in directory

```console
$ cargo-indicate
> --query-path ../indicate/test_data/queries/direct_dependencies.in.ron ../indicate/test_data/queries/count_dependencies.in.ron
> --output-dir test_target
> -- ../indicate/test_data/fake_crates/simple_deps
? success
```

## Can output `--query-dir` multiple results in directory

Ignored for now, requires GitHub token

```ignore
$ cargo-indicate
> --query-dir ../indicate/test_data/queries/
> --output-dir test_target
> -- ../indicate/test_data/fake_crates/simple_deps
? success
```
