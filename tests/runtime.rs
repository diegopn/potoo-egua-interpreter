use std::io;
use std::ops::ControlFlow;
use std::path::PathBuf;

use potoo_egua_interpreter::{Host, ModuleSource, Session, Step, check, run, run_with_host};

#[test]
fn check_and_run_support_functions_and_nested_expressions() {
    let source = "função somar(a, b) { retorna a + b; }\nescreva(somar(2, 3 * 4));";

    check(source).unwrap();
    let mut output = Vec::new();
    run(source, &mut output).unwrap();

    assert_eq!(String::from_utf8(output).unwrap(), "14\n");
}

#[test]
fn lists_can_be_mutated_and_read_by_index() {
    let mut output = Vec::new();

    run(
        "var itens = [2, 3];\nitens[0] = itens[0] + 5;\nescreva(itens[0]);",
        &mut output,
    )
    .unwrap();

    assert_eq!(String::from_utf8(output).unwrap(), "7\n");
}

#[test]
fn for_loop_runs_its_step_and_accumulates_the_expected_value() {
    let mut output = Vec::new();

    run(
        "var total = 0;\npara (var i = 1; i <= 4; i = i + 1) { total = total + i; }\nescreva(total);",
        &mut output,
    )
    .unwrap();

    assert_eq!(String::from_utf8(output).unwrap(), "10\n");
}

#[test]
fn host_can_inspect_variables_at_statement_boundaries() {
    #[derive(Default)]
    struct Inspector {
        observed: Option<(usize, Option<String>)>,
        output: Vec<String>,
    }

    impl Host for Inspector {
        fn write_line(&mut self, line: &str) -> io::Result<()> {
            self.output.push(line.to_owned());
            Ok(())
        }

        fn on_step(&mut self, step: &Step<'_>) -> ControlFlow<()> {
            if step.line == 2 {
                self.observed = Some((step.line, step.variable("total")));
            }
            ControlFlow::Continue(())
        }
    }

    let mut host = Inspector::default();
    run_with_host("var total = 8;\nescreva(total);", None, &mut host).unwrap();

    assert_eq!(host.observed, Some((2, Some("8".to_owned()))));
    assert_eq!(host.output, ["8"]);
}

#[test]
fn session_keeps_globals_between_submissions() {
    let mut session = Session::default();
    let mut output = Vec::new();
    let mut host = OutputHost(&mut output);

    session.run("var contador = 4;", None, &mut host).unwrap();
    session
        .run(
            "contador = contador + 3;\nescreva(contador);",
            None,
            &mut host,
        )
        .unwrap();

    assert_eq!(String::from_utf8(output).unwrap(), "7\n");
}

struct OutputHost<'a>(&'a mut Vec<u8>);

impl Host for OutputHost<'_> {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        use std::io::Write;
        writeln!(self.0, "{line}")
    }
}

#[test]
fn module_import_uses_the_host_and_exports_module_values() {
    struct ModuleHost(Vec<String>);

    impl Host for ModuleHost {
        fn write_line(&mut self, line: &str) -> io::Result<()> {
            self.0.push(line.to_owned());
            Ok(())
        }

        fn load_module(
            &mut self,
            _from: Option<&std::path::Path>,
            path: &str,
        ) -> io::Result<ModuleSource> {
            assert_eq!(path, "aritmetica.egua");
            Ok(ModuleSource {
                path: PathBuf::from("/virtual/aritmetica.egua"),
                source: "var exports = {\"somar\": função(a, b) { retorna a + b + bônus; }};"
                    .into(),
            })
        }
    }

    let mut host = ModuleHost(Vec::new());
    run_with_host(
        "var bônus = 3;\nvar modulo = importar(\"aritmetica.egua\");\nescreva(modulo.somar(2, 5));",
        None,
        &mut host,
    )
    .unwrap();

    assert_eq!(host.0, ["10"]);
}

#[test]
fn undefined_globals_return_a_runtime_diagnostic_with_source_location() {
    let mut output = Vec::new();
    let diagnostic = run("escreva(inexistente);", &mut output).unwrap_err();

    assert_eq!(diagnostic.phase, potoo_egua_interpreter::Phase::Runtime);
    assert_eq!(diagnostic.line, 1);
    assert!(diagnostic.column > 1);
}
