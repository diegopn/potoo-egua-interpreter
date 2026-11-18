<h1 align="center">Égua Interpreter</h1>

<p align="center">An independent Rust library and CLI for the Égua programming language.</p>

<p align="center">
  <a href="https://diegopn.github.io/potoo-site/">Website</a> ·
  <a href="https://diegopn.github.io/potoo-site/manual/egua/">User manual</a>
</p>

<p align="center">
  <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white"></a>
  <a href="https://doc.rust-lang.org/cargo/"><img alt="Cargo" src="https://img.shields.io/badge/Cargo-DEA584?style=for-the-badge&logo=rust&logoColor=white"></a>
  <img alt="Rust library" src="https://img.shields.io/badge/Rust_library-4A86CF?style=for-the-badge">
  <img alt="CLI" src="https://img.shields.io/badge/CLI-6A4C93?style=for-the-badge">
</p>

Égua is a general-purpose programming language with Portuguese syntax, first-class functions,
and object-oriented features. Pseudolanguages primarily describe algorithms for instruction.
Égua defines executable programs with classes, modules, and standard libraries.

This implementation targets [Égua v1.3.16](https://github.com/eguadev/egua/tree/v1.3.16).
See the [official language documentation](https://egua.dev/) for syntax and the
[tagged source](https://github.com/eguadev/egua/tree/v1.3.16/src) for version-specific
semantics. The [original project uses the MIT license](https://github.com/eguadev/egua/blob/v1.3.16/LICENSE).

## Command line

### Install from source

Install [Rust and Cargo](https://www.rust-lang.org/tools/install) first. This project requires Rust 1.93 or newer.

```sh
git clone --branch develop https://github.com/diegopn/potoo-egua-interpreter.git
cd potoo-egua-interpreter
cargo install --path . --locked --bin potoo-egua
```

Cargo places the executable in its configured binary directory, usually `~/.cargo/bin`. Add that directory to your `PATH` if the `potoo-egua` command is not found.

To build without installing:

```sh
cargo build --release --locked --bin potoo-egua
```

The executable is written to `target/release/` (`potoo-egua.exe` on Windows).

### Run a source file or open the console

Pass one UTF-8 source file to execute it, or run without arguments to open the interactive console:

```sh
potoo-egua programa.egua
potoo-egua
```

The console evaluates one submitted line at a time and keeps global variables in a shared session. Program output goes to standard output. Diagnostics go to standard error with a phase, line, and column and, for file execution, the source path. Passing more than one argument prints usage and returns status 64.

The CLI resolves entry-program and interactive console imports from the working directory.
While executing an imported module, it uses that module's directory. The CLI uses `curl`
on the system path for HTTP and the system's local time zone for dates. Install `curl` to use
the `requisição` HTTP functions in the CLI; requests are limited to HTTP and HTTPS and have
a 30-second timeout.

## Language coverage

- Variables, local blocks, scalar literals, vectors, dictionaries, indexing, and mutation.
- Arithmetic, comparisons, short-circuit conditions, membership, and bitwise operators.
- `se`, `senãose`, `senão`, `enquanto`, `para`, `fazer` (also `faça` before a block), `escolha`,
  `pausa`, `continua`, and `tente` with `pegue`, `senão`, and `finalmente`.
- Named and anonymous functions, closures, parameters, classes, constructors, inheritance,
  `isto`, and `super`.
- Built-in functions, file modules through `importar` and `exports`, and the
  `matemática`, `textos`, `tempo`, and `requisição` standard libraries.

The interpreter follows the tag's expression precedence and scope rules. In language
conditions, only `nulo` and `falso` are false. Collection predicates use the tagged
library's conversion of callback results.

## Embedding

Use `parse` and `ParsedProgram::resolve` to prepare source, or `check` to validate
it without execution. `run` writes output to a Rust stream. `run_with_host` accepts a
`Host` for output, imports, HTTP, time, waiting, random seeds, and execution callbacks.
`Session` retains globals between submissions and can execute a resolved `Program`.

Add a local checkout as a path dependency:

```toml
[dependencies]
potoo-egua-interpreter = { path = "../potoo-egua-interpreter" }
```

This minimal example executes a program and captures its output:

```rust
use potoo_egua_interpreter::{check, run};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "var total = 2 + 3;\nescreva(total);";
    check(source)?;

    let mut output = Vec::new();
    run(source, &mut output)?;
    print!("{}", String::from_utf8(output)?);
    Ok(())
}
```

The host can inspect variables at statement boundaries and cancel execution. The library
leaves file and HTTP access to the host and applies limits to function and import nesting.
Implement `Host::load_module` to provide file imports and `Host::request` to provide HTTP
access. `Session` is the entry point for an interactive console that retains global variables.

## Development

```sh
cargo fmt --all -- --check
cargo check --locked
cargo test --locked
```

The integration tests in `tests/` cover the public library, host inspection, imported modules,
source-file execution, diagnostics, CLI argument handling, and the persistent console.

## Support

If this project is useful to you, you can [sponsor its maintainer](https://github.com/sponsors/diegopn).

## Project policies

- [Code of conduct](CODE_OF_CONDUCT.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)
- [GPL-3.0-or-later license](LICENSE)
