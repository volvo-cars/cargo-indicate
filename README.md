<div align="center">
    <h1>🚨 cargo-indicate 🚨</h1>
    <i>Run GraphQL Queries on Your Dependency Graph</i>
</div>
<br />

[![Crates.io (cargo-indicate)](https://img.shields.io/crates/v/cargo-indicate)](https://crates.io/crates/cvars)

<br />

This is the result of a Master's thesis written at LTH in collaboration with
Volvo Cars by [Emil Eriksson](github.com/ginger51011).

To get started, install `cargo-indicate` using

```
cargo install cargo-indicate
```

and check out [the `cargo-indicate` docs](./cargo-indicate/README.md).

While `cargo-indicate` allows for experimenting, it might be a good idea to read
the conclusions in this thesis, as they provide guidance and context on how to
interpret the results, and provides context. The thesis also includes
explanation of the code and design decisions.

This project relies heavily on
[`trustfall`](https://github.com/obi1kenobi/trustfall), the query engine behind
[`cargo-semver-checks`](https://github.com/obi1kenobi/cargo-semver-checks).

## Project Structure

- [`indicate`](./indicate) is the library providing central functionality
- [`cargo-indicate`](./cargo-indicate/) is the cargo add-on itself

## Caching of HTTP requests

While `indicate` will cache already made requests during one run, it will also
use the GitHub HTTP cache system, where ETags are used to verify if an API
request has changed since it was last made (perhaps in another invocation of
`indicate`). If it receives a `304 Not Changed`, it will use the `~/.github/`
directory to retrieve a cached version.
