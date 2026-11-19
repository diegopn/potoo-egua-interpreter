# Égua reference coverage

This crate targets Égua v1.3.16. It implements a source interpreter and CLI; it does not reproduce the upstream editor or JavaScript runtime internals in full.

| Reference area | Implemented profile | Boundary or validation status |
| --- | --- | --- |
| Syntax and expressions | The pinned source grammar, statements, operators, precedence, truthiness, and nested expressions | Syntax and runtime behavior are covered by the CLI, runtime, and conformance tests; no whole-suite differential run against v1.3.16 is claimed. |
| Values and collections | Numbers, text, booleans, null-like values, arrays/lists, dictionaries, indexing, mutation, and documented conversion behavior | JavaScript number coercion/formatting and UTF-16 string boundaries are covered by focused regression tests. |
| Functions and control | First-class functions, closures, recursion, classes, methods, conditionals, loops, and returns | Function, call, import, and serialization depth limits are interpreter safety policies. |
| Modules and libraries | Module resolution, exported values, tagged standard-library operations, and session globals | Imports are supplied through the resolver or host; the CLI uses the working directory for entry and console imports, then each imported module's directory. |
| Host-dependent behavior | Output, HTTP, dates, delays, randomness, and module loading are exposed through Host | The CLI uses system curl and local time-zone data for HTTP/date operations. HTTP requests are limited to HTTP(S) with a 30-second timeout. |
| Diagnostics | Parse, resolution, and runtime errors retain phase, source location, and path where available | The original runtime's behavior is not inferred where the pinned source does not establish it. |

Existing representative coverage lives in tests/conformance.rs, tests/runtime.rs, and tests/cli.rs.

## Sources

- [Official Égua documentation](https://egua.dev/)
- [Égua v1.3.16 source](https://github.com/eguadev/egua/tree/v1.3.16/src)
- [Égua v1.3.16 license](https://github.com/eguadev/egua/blob/v1.3.16/LICENSE)
