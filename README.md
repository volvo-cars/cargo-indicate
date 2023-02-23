# cargo-indicate 🚨

A tool for querying different signals for risks in your dependency tree.

- [`indicate`](./indicate) is the library providing central functionality
- [`cargo-indicate`](./cargo-indicate/) is the cargo add-on itself

## Caching of HTTP requests

While `indicate` will cache already made requests during one run, it will also
use the GitHub HTTP cache system, where ETags are used to verify if an API
request has changed since it was last made (perhaps in another invocation of
`indicate`). If it receives a `304 Not Changed`, it will use the `~/.github/`
directory to retrieve a cached version.