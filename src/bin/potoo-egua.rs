#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use potoo_egua_interpreter::{
    Host, HttpRequest, HttpResponse, ModuleSource, Phase, Session, run_with_host,
};

struct Console<W>(W);

impl<W: Write> Host for Console<W> {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        writeln!(self.0, "{line}")
    }

    fn load_module(&mut self, from: Option<&Path>, path: &str) -> io::Result<ModuleSource> {
        let base = from
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."));
        let path = base.join(path);
        let source = fs::read_to_string(&path)?;
        Ok(ModuleSource { path, source })
    }

    fn entry_import_path<'path>(&self, _path: Option<&'path Path>) -> Option<&'path Path> {
        None
    }

    fn request(&mut self, request: &HttpRequest<'_>) -> io::Result<HttpResponse> {
        let mut command = Command::new("curl");
        command.args([
            "-q",
            "--silent",
            "--show-error",
            "--globoff",
            "--max-time",
            "30",
            "--proto",
            "=http,https",
            "--request",
            request.method,
            "--url",
            request.url,
            "--write-out",
            "\n%{http_code}",
        ]);
        if request.body.is_some() {
            command.args([
                "--header",
                "Content-Type: application/json",
                "--data-binary",
                "@-",
            ]);
        }
        let mut process = command
            .stdin(if request.body.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let writer = process.stdin.take().zip(request.body);
        let output = std::thread::scope(|scope| {
            let writing = writer
                .map(|(mut input, body)| scope.spawn(move || input.write_all(body.as_bytes())));
            let output = process.wait_with_output()?;
            if output.status.success()
                && let Some(writing) = writing
            {
                writing
                    .join()
                    .map_err(|_| io::Error::other("Erro ao enviar o corpo da requisição."))??;
            }
            Ok::<_, io::Error>(output)
        })?;
        if !output.status.success() {
            return Err(io::Error::other(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        let response = String::from_utf8_lossy(&output.stdout);
        let (body, status) = response
            .rsplit_once('\n')
            .ok_or_else(|| io::Error::other("Resposta HTTP sem código de status."))?;
        let status = status.parse().map_err(io::Error::other)?;
        Ok(HttpResponse {
            status,
            body: body.to_owned(),
        })
    }
}

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let path = arguments.next().map(PathBuf::from);
    if arguments.next().is_some() {
        eprintln!("Uso: potoo-egua [arquivo.egua]");
        return ExitCode::from(64);
    }

    let mut console = Console(io::stdout().lock());
    let result = if let Some(path) = path {
        Ok(run_file(&path, &mut console))
    } else {
        run_prompt(&mut console).map(|()| ExitCode::SUCCESS)
    };
    match result.and_then(|status| console.0.flush().map(|()| status)) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("Erro de E/S: {error}");
            ExitCode::from(70)
        }
    }
}

fn run_file(path: &Path, console: &mut Console<impl Write>) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!(
                "{}: não foi possível ler o arquivo: {error}",
                path.display()
            );
            return ExitCode::from(66);
        }
    };
    match run_with_host(&source, Some(path), console) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(match error.phase {
                Phase::Runtime => 70,
                Phase::Cancelled => 130,
                _ => 65,
            })
        }
    }
}

fn run_prompt(console: &mut Console<impl Write>) -> io::Result<()> {
    console.write_line("Console da Linguagem Égua")?;
    let mut session = Session::default();
    let mut input = io::stdin().lock();
    let mut line = String::new();
    loop {
        write!(console.0, "\négua> ")?;
        console.0.flush()?;
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        if let Err(error) = session.run(&line, None, console) {
            eprintln!("{error}");
        }
        line.clear();
    }
}
