#![forbid(unsafe_code)]

//! UI-independent interpreter for the Égua v1.3.16 programming language.

mod builtins;
mod lexer;
mod parser;
mod resolver;
mod runtime;
mod value;

use std::fmt::{self, Display, Formatter};
use std::io::{self, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Local, Utc};

/// Phase in which a program stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    Lexical,
    Syntax,
    Resolution,
    Runtime,
    Cancelled,
}

/// An error with a one-based source line and column.
#[derive(Debug)]
pub struct Diagnostic {
    pub phase: Phase,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug)]
struct Location {
    line: usize,
    column: usize,
}

impl Location {
    fn error(self, phase: Phase, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            phase,
            line: self.line,
            column: self.column,
            message: message.into(),
            path: None,
        }
    }
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let phase = match self.phase {
            Phase::Lexical => "erro léxico",
            Phase::Syntax => "erro sintático",
            Phase::Resolution => "erro de resolução",
            Phase::Runtime => "erro de execução",
            Phase::Cancelled => "execução interrompida",
        };
        if let Some(path) = &self.path {
            write!(f, "{}:", path.display())?;
        }
        write!(
            f,
            "{}:{}: {phase}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for Diagnostic {}

/// A parsed program. Use [ParsedProgram::resolve] before executing it.
pub struct ParsedProgram(Vec<parser::Stmt>);

impl ParsedProgram {
    /// Resolves lexical bindings and checks contextual restrictions.
    pub fn resolve(mut self) -> Result<Program, Diagnostic> {
        resolver::resolve(&mut self.0)?;
        Ok(Program(self.0))
    }
}

/// A resolved program that can be executed in one or more sessions.
pub struct Program(Vec<parser::Stmt>);

/// Tokenizes and parses UTF-8 Égua source.
pub fn parse(source: &str) -> Result<ParsedProgram, Diagnostic> {
    parser::parse(&lexer::scan(source)?).map(ParsedProgram)
}

/// Checks syntax and resolves lexical bindings.
pub fn check(source: &str) -> Result<(), Diagnostic> {
    parse(source)?.resolve().map(|_| ())
}

/// The next statement location and the active execution context.
pub struct Step<'a> {
    pub path: Option<&'a Path>,
    pub line: usize,
    pub column: usize,
    pub routine: Option<&'a str>,
    pub call_depth: usize,
    inspect: &'a dyn Fn(&str) -> Option<String>,
}

impl Step<'_> {
    /// Returns the Égua display value of a variable visible in the current environment.
    pub fn variable(&self, name: &str) -> Option<String> {
        (self.inspect)(name)
    }
}

/// Source loaded by the host for a file import. The path identifies the module in
/// diagnostics and is passed back to the host when the module imports another file.
pub struct ModuleSource {
    pub path: PathBuf,
    pub source: String,
}

/// The request to send for a function in `requisição`.
pub struct HttpRequest<'a> {
    pub method: &'static str,
    pub url: &'a str,
    pub body: Option<&'a str>,
}

/// The response received for a function in `requisição`.
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Platform services for a running program.
pub trait Host {
    /// Writes a line produced by `escreva`.
    fn write_line(&mut self, line: &str) -> io::Result<()>;

    /// Resolves a file import. The entry program passes no module path.
    /// An imported module passes the path returned by the host for that module.
    fn load_module(&mut self, _from: Option<&Path>, _path: &str) -> io::Result<ModuleSource> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Importação de arquivos indisponível.",
        ))
    }

    /// Sends an HTTP request. The host controls which destinations are available.
    fn request(&mut self, _request: &HttpRequest<'_>) -> io::Result<HttpResponse> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Requisições HTTP indisponíveis.",
        ))
    }

    /// Reports statement locations, allowing a debugger to pause or cancel.
    fn on_step(&mut self, _step: &Step<'_>) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    /// Allows the host to cancel between statements and during library iterations.
    fn cancelled(&mut self) -> bool {
        false
    }

    /// Supplies the current time.
    fn now(&mut self) -> DateTime<Utc> {
        Utc::now()
    }

    /// Supplies the local UTC offset at the specified instant.
    fn local_offset(&mut self, instant: DateTime<Utc>) -> FixedOffset {
        *instant.with_timezone(&Local).fixed_offset().offset()
    }

    /// Waits for a duration. Hosts may interrupt by returning `ErrorKind::Interrupted`.
    fn wait(&mut self, duration: Duration) -> io::Result<()> {
        std::thread::sleep(duration);
        Ok(())
    }

    /// Supplies the initial seed for the interpreter's random generator.
    fn random_seed(&mut self) -> u64 {
        Utc::now().timestamp_nanos_opt().unwrap_or_default() as u64
    }
}

struct OutputHost<'a, W>(&'a mut W);

impl<W: Write> Host for OutputHost<'_, W> {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        writeln!(self.0, "{line}")
    }
}

/// Runs source in a fresh session and writes console output to a stream.
pub fn run(source: &str, output: &mut impl Write) -> Result<(), Diagnostic> {
    run_with_host(source, None, &mut OutputHost(output))
}

/// Runs source in a fresh session using host services.
pub fn run_with_host(
    source: &str,
    path: Option<&Path>,
    host: &mut dyn Host,
) -> Result<(), Diagnostic> {
    Session::default().run(source, path, host)
}

/// A sequence of executions that share global variables, suitable for a console.
#[derive(Default)]
pub struct Session(runtime::State);

impl Session {
    /// Parses, resolves, and executes source in this session.
    pub fn run(
        &mut self,
        source: &str,
        path: Option<&Path>,
        host: &mut dyn Host,
    ) -> Result<(), Diagnostic> {
        let program = parse(source)
            .and_then(ParsedProgram::resolve)
            .map_err(|mut error| {
                error.path = path.map(Path::to_owned);
                error
            })?;
        self.execute(&program, path, host)
    }

    /// Executes an already resolved program in this session.
    pub fn execute(
        &mut self,
        program: &Program,
        path: Option<&Path>,
        host: &mut dyn Host,
    ) -> Result<(), Diagnostic> {
        self.0.execute(&program.0, path, host)
    }
}
