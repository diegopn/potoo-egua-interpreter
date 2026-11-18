<p align="center">
  <img src="https://diegopn.github.io/potoo-site/src/assets/images/language-interpreter-icon.svg?v=1c651ca7" alt="Potoo IDE language interpreter icon" width="120" />
</p>

<h1 align="center">Égua Interpreter</h1>

<p align="center">An independent Rust interpreter library and CLI for the Égua programming language.</p>

<p align="center">
  <a href="https://github.com/diegopn/potoo-egua-interpreter/releases/latest">Download</a> ·
  <a href="https://diegopn.github.io/potoo-site/">Website</a> ·
  <a href="https://diegopn.github.io/potoo-site/manual/egua/">User manual</a>
</p>

<p align="center">
  <a href="https://www.rust-lang.org/"><img alt="Rust" src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white"></a>
  <a href="https://doc.rust-lang.org/cargo/"><img alt="Cargo" src="https://img.shields.io/badge/Cargo-DEA584?style=for-the-badge&logo=rust&logoColor=white"></a>
  <img alt="Rust library" src="https://img.shields.io/badge/Rust_library-4A86CF?style=for-the-badge">
  <img alt="CLI" src="https://img.shields.io/badge/CLI-6A4C93?style=for-the-badge">
</p>

Originally developed as part of the Potoo IDE project, this interpreter is an independent Rust software package intended to be distributed separately from the IDE. It can be embedded as a library in other Rust projects or run as a standalone command-line tool on Linux, macOS, and Windows.

Égua is an interpreted programming language with Portuguese syntax, designed for teaching programming. It has first-class functions, classes, modules, and standard libraries. This interpreter targets [Égua v1.3.16](https://github.com/eguadev/egua/tree/v1.3.16).

## Original language references

- [Official Égua language documentation](https://egua.dev/)
- [Égua v1.3.16 source](https://github.com/eguadev/egua/tree/v1.3.16/src)
- [Égua v1.3.16 MIT license](https://github.com/eguadev/egua/blob/v1.3.16/LICENSE)

## Source example

```egua
var total = 2 + 3;
escreva(total);
```

## Standalone command-line runner

### Install from source

Install [Rust and Cargo](https://www.rust-lang.org/tools/install) first. This project requires Rust 1.93 or newer.

```sh
git clone --branch main https://github.com/diegopn/potoo-egua-interpreter.git
cd potoo-egua-interpreter
cargo install --path . --locked --bin potoo-egua
```

Cargo places the executable in its configured binary directory, usually `~/.cargo/bin`. Add that directory to your `PATH` if the `potoo-egua` command is not found.

To build without installing:

```sh
cargo build --release --locked --bin potoo-egua
```

The executable is written to `target/release/` (`potoo-egua.exe` on Windows).

### Prebuilt CLI packages

GitHub releases provide `x86_64-unknown-linux-gnu` (`.tar.gz`), `x86_64-pc-windows-msvc` (`.zip`, containing `potoo-egua.exe`), and `x86_64-apple-darwin` (`.tar.gz`, Intel macOS) packages. There is no Apple Silicon build.

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

## Use as a Rust library

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
leaves file and HTTP access to the host: implement `Host::load_module` for file imports and
`Host::request` for HTTP. It caps function-call nesting at 128, file-import nesting at 64,
and HTTP JSON nesting at 128 levels to bound runaway programs and serialization work.
`Session` is the entry point for an interactive console that retains global variables.

## Development

```sh
cargo fmt --all -- --check
cargo check --locked
cargo test --locked
```

## Support

If this project is useful to you, you can [sponsor its maintainer](https://github.com/sponsors/diegopn).

## Project policies

- [Code of conduct](CODE_OF_CONDUCT.md)
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)
- [GPL-3.0-or-later license](LICENSE)
