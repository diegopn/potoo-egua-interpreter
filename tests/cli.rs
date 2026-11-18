use std::fs;
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const CLI: &str = env!("CARGO_BIN_EXE_potoo-egua");

struct ProgramDirectory(std::path::PathBuf);

impl ProgramDirectory {
    fn new() -> Self {
        let unique = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("potoo-egua-{}-{unique}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        Self(directory)
    }

    fn write(&self, name: &str, source: &str) -> std::path::PathBuf {
        let path = self.0.join(name);
        fs::write(&path, source).unwrap();
        path
    }
}

impl Drop for ProgramDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(path: &std::path::Path) -> Output {
    Command::new(CLI).arg(path).output().unwrap()
}

#[test]
fn cli_executes_a_source_file_and_writes_program_output() {
    let directory = ProgramDirectory::new();
    let path = directory.write("ola.egua", "escreva(\"Olá, Égua!\");");

    let output = run(&path);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "Olá, Égua!\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn cli_reports_phase_location_and_path_for_invalid_source() {
    let directory = ProgramDirectory::new();
    let path = directory.write("invalido.egua", "escreva(;");

    let output = run(&path);

    assert_eq!(output.status.code(), Some(65));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalido.egua:"));
    assert!(stderr.contains("erro sintático"));
}

#[test]
fn cli_requires_at_most_one_source_file() {
    let output = Command::new(CLI)
        .args(["um.egua", "dois.egua"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Uso: potoo-egua"));
}

#[test]
fn interactive_console_reuses_variables_across_input_lines() {
    let mut child = Command::new(CLI)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"var contador = 6;\nescreva(contador);\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Console da Linguagem Égua"));
    assert!(stdout.contains("6\n"));
    assert!(stdout.contains("égua>"));
}

#[test]
fn cli_resolves_nested_imports_from_each_module_directory() {
    let directory = ProgramDirectory::new();
    fs::create_dir_all(directory.0.join("pkg")).unwrap();
    directory.write(
        "main.egua",
        "var modulo = importar(\"pkg/modulo.egua\");\nescreva(modulo.somar(2, 5));",
    );
    directory.write("pkg/modulo.egua", "var valores = importar(\"valores.egua\");\nexports = {\"somar\": função(a, b) { retorna a + b + valores.bônus; }};");
    directory.write("pkg/valores.egua", "exports = {\"bônus\": 10};");

    let output = Command::new(CLI)
        .current_dir(&directory.0)
        .arg("main.egua")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "17\n");
}
