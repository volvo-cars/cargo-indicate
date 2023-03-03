# `cargo-indicate` Run queries against your dependency tree

You can install the custom command using

```ignore
cargo install --path .
```

which allows you to call the command using `cargo indicate`.

Run the following for help

```ignore
cargo indicate --help # or -h
```

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