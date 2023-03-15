## Cannot output into directory when using `-q` as input
```console
$ cargo-indicate -q '' --output-dir test_target/will_fail
? failed
error: the argument '--query <QUERY>' cannot be used with '--output-dir <DIR>'

Usage: cargo-indicate <--query-path <FILE>|--query-dir <DIR>|--query <QUERY>> [PACKAGE]

For more information, try '--help'.

```
