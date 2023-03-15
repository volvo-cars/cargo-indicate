## Using both `--output` and `--output-dir` fails

```console
$ cargo-indicate --output test_target/output.out.json --output-dir test_target/will_fail
? failed
error: the argument '--output <FILE>' cannot be used with '--output-dir <DIR>'

Usage: cargo-indicate --output <FILE> <--query-path <FILE>|--query-dir <DIR>|--query <QUERY>> [PACKAGE]

For more information, try '--help'.

```

