use std::io;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use potoo_egua_interpreter::{
    Host, HttpRequest, HttpResponse, ModuleSource, Phase, run, run_with_host,
};

fn output(source: &str) -> String {
    let mut bytes = Vec::new();
    run(source, &mut bytes).unwrap();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn number_coercion_matches_javascript_radix_and_whitespace_rules() {
    let source = "var m = importar(\"matemática\");\nescreva(m.graus(\"0x10000000000000000\") > 100000000000000000000);";

    assert_eq!(output(source), "verdadeiro\n");
}

#[test]
fn number_coercion_trims_javascript_byte_order_mark_whitespace() {
    let byte_order_mark = '\u{feff}';
    let source = format!(
        "var m = importar(\"matemática\");\nescreva(m.graus(\"{byte_order_mark}2\") == m.graus(2));"
    );

    assert_eq!(output(&source), "verdadeiro\n");
}

#[test]
fn text_case_conversion_preserves_unicode_context_and_surrogate_units() {
    let source = "var textos = importar(\"textos\");\nescreva(textos.minúsculo(\"ΟΣ\"));\nvar unidade = textos.caractere_em(\"😀\", 0);\nescreva(textos.código_em(textos.maiúsculo(unidade), 0));\nescreva(textos.código_em(texto(unidade), 0));\nescreva(textos.código_em(textos.formate(\"{}\", unidade), 0));";

    assert_eq!(output(source), "ος\n55357\n55357\n55357\n");
}

#[test]
fn collection_callback_observes_items_appended_during_forward_iteration() {
    let source = "var itens = [1, 2];\nvar visitas = 0;\nfunção adicionar_item(valor) { visitas = visitas + 1; se (visitas == 1) { itens[2] = 3; } retorna valor; }\nmapear(itens, adicionar_item);\nescreva(visitas);";

    assert_eq!(output(source), "3\n");
}

#[test]
fn dictionary_membership_includes_javascript_object_prototype_names() {
    let source = "escreva(\"toString\" em {});\nescreva(\"__proto__\" em {});\nescreva(\"nome\" em {\"nome\": nulo});";

    assert_eq!(output(source), "verdadeiro\nverdadeiro\nverdadeiro\n");
}

#[test]
fn dictionary_membership_rejects_values_that_override_the_prototype_constructor() {
    let source = "var d = {\"constructor\": nulo};\ntente { escreva(\"nome\" em d); } pegue { escreva(\"erro\"); }";

    assert_eq!(output(source), "erro\n");
}

#[test]
fn imported_module_syntax_errors_keep_the_module_path() {
    struct InvalidModuleHost;

    impl Host for InvalidModuleHost {
        fn write_line(&mut self, _line: &str) -> io::Result<()> {
            Ok(())
        }

        fn load_module(
            &mut self,
            _from: Option<&std::path::Path>,
            _path: &str,
        ) -> io::Result<ModuleSource> {
            Ok(ModuleSource {
                path: "pkg/invalid.egua".into(),
                source: "escreva(;".to_owned(),
            })
        }
    }

    let mut host = InvalidModuleHost;
    let error = run_with_host(
        "importar(\"pkg/invalid.egua\");",
        Some(std::path::Path::new("main.egua")),
        &mut host,
    )
    .unwrap_err();

    assert_eq!(error.phase, Phase::Syntax);
    assert_eq!(
        error.path.as_deref(),
        Some(std::path::Path::new("pkg/invalid.egua"))
    );
}

#[test]
fn tagged_language_examples_cover_precedence_truthiness_closures_classes_and_flow() {
    let source = "\
escreva(2 + 3 * 4);\n\
escreva(2 ** 3 ** 2);\n\
escreva(-2 ** 2);\n\
escreva(0);\n\
escreva(\"\");\n\
escreva([]);\n\
se (nulo) escreva(\"incorreto\"); senão escreva(\"nulo-falso\");\n\
se (0) escreva(\"zero-verdadeiro\");\n\
se (\"\") escreva(\"texto-vazio-verdadeiro\");\n\
função construir_somador(base) { var local = base; retorna função(valor) { retorna local + valor; }; }\n\
var somar = construir_somador(7);\n\
escreva(somar(3));\n\
classe A { construtor(valor) { isto.valor = valor; retorna 99; } ler() { retorna isto.valor; } }\n\
classe B herda A { ler() { retorna super.ler() + 1; } }\n\
escreva(B(4).ler());\n\
var total = 0; para (var i = 0; i < 5; i = i + 1) { se (i == 2) continua; total = total + i; }\n\
fazer { total = total + 1; } enquanto (falso);\n\
escolha (total) { caso 9: escreva(\"fluxo-ok\"); padrão: escreva(\"incorreto\"); }\n\
tente { 1 + \"texto\"; } pegue { escreva(\"erro-capturado\"); } finalmente { escreva(\"finalmente\"); }";

    assert_eq!(
        output(source),
        "14\n64\n4\n0\n\n[]\nnulo-falso\nzero-verdadeiro\ntexto-vazio-verdadeiro\n10\n5\nfluxo-ok\nerro-capturado\nfinalmente\n"
    );
}

#[test]
fn tagged_math_examples_and_numeric_boundaries_are_covered() {
    let source = "\
var m = importar(\"matemática\");\n\
escreva(m.graus(m.pi) == 180);\n\
escreva(m.pontos_espaçados(0, 10, 5));\n\
escreva(m.moda([1, 2, 2, 3]));\n\
escreva(m.aprox([1.2345, 2.3456], 2));\n\
escreva(m.aprox(1.005, 2));\n\
escreva(m.aprox(2.5, 0));\n\
escreva(m.fatorial(0));\n\
escreva(m.fatorial(171));\n\
escreva(m.max([]));\n\
escreva(m.min([]));\n\
escreva(m.eh_par(-2));\n\
escreva(m.eh_impar(0));\n\
tente { m.mediana([]); } pegue { escreva(\"vetor-vazio\"); }";

    assert_eq!(
        output(source),
        "verdadeiro\n[ 0, 2.5, 5, 7.5, 10 ]\n[ 2 ]\n[ 1.23, 2.35 ]\n1\n3\n1\nInfinity\n-Infinity\nInfinity\nverdadeiro\nfalso\nvetor-vazio\n"
    );
}

#[test]
fn tagged_math_examples_cover_every_exported_operation() {
    let source = "\
var m = importar(\"matemática\");\n\
escreva(m.radiano(180) > 3.14);\n\
escreva(m.gerar_pontos_abscissa(2, 10, 5));\n\
escreva(m.fun1R(2, -4));\n\
escreva(m.fun2R(1, -5, 6));\n\
escreva(m.número_ocorrências([1, 2, 2], 2));\n\
escreva(m.média([2, 4, 6]));\n\
escreva(m.sen(m.pi / 2) > 0.999999);\n\
escreva(m.cos(0));\n\
escreva(m.tan(0));\n\
escreva(m.arcos(1));\n\
escreva(m.arc_sen(0));\n\
escreva(m.arc_tan(0));\n\
escreva(m.exp(0));\n\
escreva(m.log(1));\n\
escreva(m.potência(2, 10));\n\
escreva(m.raizq(9));\n\
escreva(m.velocidade_média(10, 2));\n\
escreva(m.delta_s(2, 10));\n\
escreva(m.delta_t(2, 10));\n\
escreva(m.aceleração(10, 2, 5, 1));\n\
escreva(m.mrufh(1, 2, 3));\n\
escreva(m.mruv(1, 3, 2));\n\
escreva(m.min_aprox(-1.2));\n\
escreva(m.arredondar(-2.5));\n\
escreva(m.teto(4.2));\n\
escreva(m.absoluto(-7));\n\
escreva(m.sinal(-7));\n\
escreva(m.raiz_cubica(-8));\n\
escreva(m.log_base2(8));\n\
escreva(m.log_base10(100));\n\
escreva(m.sen_hiperbolico(0));\n\
escreva(m.cos_hiperbolico(0));\n\
escreva(m.tan_hiperbolico(0));\n\
escreva(m.somatorio([1, 2, 3]));\n\
escreva(m.produto([2, 3, 4]));";

    assert_eq!(
        output(source),
        "verdadeiro\n[ 6, 8, 10, 12, 14 ]\n2\n[ 3, 2 ]\n2\n4\nverdadeiro\n1\n0\n0\n0\n0\n1\n0\n1024\n3\n5\n8\n8\n2\n[ \"Função: 1+(2)*t\\nPosições: 1,3,5,7\" ]\n[ 1, 3, 5 ]\n-2\n-2\n5\n7\n-1\n-2\n3\n2\n0\n1\n0\n6\n24\n"
    );
}

#[test]
fn tagged_text_examples_cover_all_exports_and_utf16_boundaries() {
    let source = "\
var t = importar(\"textos\");\n\
escreva(t.letras_minúsculas);\n\
escreva(t.letras_maiúsculas);\n\
escreva(t.letras);\n\
escreva(t.dígitos);\n\
escreva(t.pontuações);\n\
escreva(t.maiúsculo(\"égua\"));\n\
escreva(t.minúsculo(\"ÉGUA\"));\n\
escreva(t.contém(\"linguagem égua\", \"égua\"));\n\
escreva(t.formate(\"Olá, {}! Você tem {} anos.\", \"Ana\", 30));\n\
escreva(t.dividir(\"um,dois,três\", \",\"));\n\
escreva(t.dividir(\"ab\", \",\", 0));\n\
escreva(t.tamanho(\"😀\"));\n\
escreva(t.repita(\"ab\", 3));\n\
escreva(t.substituir(\"banana\", \"a\", \"o\"));\n\
escreva(t.substituir_tudo(\"banana\", \"a\", \"o\"));\n\
escreva(t.busca(\"linguagem égua\", \"égua\"));\n\
escreva(t.apara(\"  égua  \"));\n\
escreva(t.código_em(\"😀\", 1));\n\
escreva(t.fatiar(\"programação\", 0, 7));\n\
escreva(t.começa_com(\"égua linguagem\", \"égua\"));\n\
escreva(t.termina_com(\"linguagem égua\", \"égua\"));\n\
escreva(t.preencha_início(\"42\", 5, \"0\"));\n\
escreva(t.preencha_fim(\"42\", 5, \".\"));";

    assert_eq!(
        output(source),
        "abcdefghijklmnopqrstuvwxyz\nABCDEFGHIJKLMNOPQRSTUVWXYZ\nabcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ\n0123456789\n!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~\nÉGUA\négua\nverdadeiro\nOlá, Ana! Você tem 30 anos.\n[ \"um\", \"dois\", \"três\" ]\n[]\n2\nababab\nbonana\nbonono\n10\négua\n56832\nprogram\nverdadeiro\nverdadeiro\n00042\n42...\n"
    );
}

#[test]
fn tagged_text_split_defaults_to_utf16_code_units() {
    assert_eq!(
        output("var t = importar(\"textos\"); escreva(t.dividir(\"ab\"));"),
        "[ \"a\", \"b\" ]\n"
    );
}

#[test]
fn global_collection_helpers_cover_documented_callbacks_and_empty_cases() {
    let source = "\
var vistos = 0;\n\
função duplicar(valor) { retorna valor * 2; }\n\
função acima_de_um(valor) { retorna valor > 1; }\n\
função retorna_zero(valor) { retorna 0; }\n\
função somar(acumulado, valor) { retorna acumulado + valor; }\n\
para_cada([1, 2, 3], função(valor) { vistos = vistos + valor; });\n\
escreva(vistos);\n\
escreva(mapear([1, 2, 3], duplicar));\n\
escreva(filtrar([1, 2, 3, 4], acima_de_um));\n\
escreva(reduzir([1, 2, 3], somar, 0));\n\
escreva(reduzir([1, 2, 3], somar));\n\
escreva(encontrar([1, 2, 3], acima_de_um));\n\
escreva(encontrar_último([1, 2, 3], acima_de_um));\n\
escreva(encontrar_índice([1, 2, 3], acima_de_um));\n\
escreva(encontrar_último_índice([1, 2, 3], acima_de_um));\n\
escreva(incluído([1, 2, 3], 3));\n\
escreva(algum([], acima_de_um));\n\
escreva(todos([], acima_de_um));\n\
escreva(algum([1], retorna_zero));\n\
escreva(todos([1], retorna_zero));\n\
var desordenados = [3, 1, 2]; ordenar(desordenados); escreva(desordenados);\n\
escreva(inteiro(\"12.9\"));\n\
escreva(real(\"-12\"));\n\
escreva(tamanho(função(a, b) {}));\n\
escreva(tamanho(aleatório_entre));\n\
escreva(texto([1, 2]));\n\
var amostra = aleatório(); escreva(amostra >= 0 e amostra < 1);\n\
var inteiro_aleatório = aleatório_entre(1, 9); escreva(inteiro_aleatório >= 1 e inteiro_aleatório < 9);";

    assert_eq!(
        output(source),
        "6\n[ 2, 4, 6 ]\n[ 2, 3, 4 ]\n6\n6\n2\n3\n1\n2\nverdadeiro\nfalso\nverdadeiro\nfalso\nfalso\n[ 1, 2, 3 ]\n12\n-12\n2\n1\n1,2\nverdadeiro\nverdadeiro\n"
    );
}

#[derive(Clone)]
struct RecordedRequest {
    method: String,
    url: String,
    body: Option<String>,
}

struct DeterministicHost {
    output: Vec<String>,
    instant: DateTime<Utc>,
    offset: FixedOffset,
    waits: Vec<Duration>,
    requests: Vec<RecordedRequest>,
    seed_calls: usize,
}

impl Host for DeterministicHost {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.output.push(line.to_owned());
        Ok(())
    }

    fn now(&mut self) -> DateTime<Utc> {
        self.instant
    }

    fn local_offset(&mut self, _instant: DateTime<Utc>) -> FixedOffset {
        self.offset
    }

    fn wait(&mut self, duration: Duration) -> io::Result<()> {
        self.waits.push(duration);
        Ok(())
    }

    fn random_seed(&mut self) -> u64 {
        self.seed_calls += 1;
        42
    }

    fn request(&mut self, request: &HttpRequest<'_>) -> io::Result<HttpResponse> {
        self.requests.push(RecordedRequest {
            method: request.method.to_owned(),
            url: request.url.to_owned(),
            body: request.body.map(str::to_owned),
        });
        Ok(HttpResponse {
            status: 200,
            body: "{\"ok\":true}".to_owned(),
        })
    }
}

#[test]
fn host_supplies_local_clock_wait_random_seed_and_http_transport() {
    let source = "\
var tempo = importar(\"tempo\");\n\
var agora = tempo.agora();\n\
escreva(tempo.formate_data(agora, \"DD/MM/AAAA\"));\n\
escreva(tempo.diferença(tempo.texto_para_data(\"29/02/2024\"), tempo.texto_para_data(\"01/03/2024\")));\n\
escreva(tempo.diferenca(tempo.texto_para_data(\"29/02/2024\"), tempo.texto_para_data(\"01/03/2024\")));\n\
escreva(tempo.segundos());\n\
escreva(tempo.minutos());\n\
escreva(tempo.horas());\n\
escreva(tempo.dia());\n\
escreva(tempo.mes());\n\
escreva(tempo.ano());\n\
escreva(tempo.dia_da_semana());\n\
tente { tempo.texto_para_data(\"29/02/2023\"); } pegue { escreva(\"data-inválida\"); }\n\
tente { tempo.pausa(-1); } pegue { escreva(\"pausa-inválida\"); }\n\
tempo.pausa(1.2);\n\
var req = importar(\"requisição\");\n\
var t = importar(\"textos\");\n\
escreva(req.obter(\"api.example/items\").ok);\n\
escreva(req.enviar(\"https://api.example/items\", {\"nome\": \"Égua\"}).ok);\n\
escreva(req.enviar(\"https://api.example/surrogate\", {\"u\": t.caractere_em(\"😀\", 0)}).ok);\n\
escreva(req.enviar(\"https://api.example/small\", {\"n\": 0.0000001}).ok);\n\
escreva(req.enviar(\"https://api.example/negative-zero\", {\"n\": -0.0}).ok);\n\
escreva(req.enviar(\"https://api.example/function\", {\"fn\": t.tamanho}).ok);\n\
escreva(req.atualizar(\"https://api.example/items/1\", {\"n\": 2}).ok);\n\
escreva(req.excluir(\"https://api.example/items/1\").ok);\n\
escreva(req.enviar(\"https://api.example/null\", nulo).ok);\n\
escreva(req.atualizar(\"https://api.example/text\", \"texto\").ok);\n\
escreva(req.enviar(\"https://api.example/nonfinite\", {\"n\": 1 / 0}).ok);\n\
var aleatorio = aleatório(); escreva(aleatorio >= 0 e aleatorio < 1);\n\
var sorteado = aleatório_entre(1, 9); escreva(sorteado >= 1 e sorteado < 9);";
    let mut host = DeterministicHost {
        output: Vec::new(),
        instant: Utc.with_ymd_and_hms(2024, 2, 29, 12, 34, 56).unwrap(),
        offset: FixedOffset::west_opt(3 * 60 * 60).unwrap(),
        waits: Vec::new(),
        requests: Vec::new(),
        seed_calls: 0,
    };

    run_with_host(source, None, &mut host).unwrap();

    assert_eq!(
        host.output,
        [
            "29/02/2024",
            "1",
            "1",
            "56",
            "34",
            "9",
            "29",
            "2",
            "2024",
            "quinta-feira",
            "data-inválida",
            "pausa-inválida",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
            "verdadeiro",
        ]
    );
    assert_eq!(host.waits, [Duration::from_millis(2)]);
    assert_eq!(host.seed_calls, 1);
    assert_eq!(
        host.requests
            .iter()
            .map(|request| request.method.as_str())
            .collect::<Vec<_>>(),
        [
            "GET", "POST", "POST", "POST", "POST", "POST", "PUT", "DELETE", "POST", "PUT", "POST"
        ]
    );
    assert_eq!(host.requests[0].url, "https://api.example/items");
    assert_eq!(
        host.requests[1].body.as_deref(),
        Some("{\"nome\":\"Égua\"}")
    );
    assert_eq!(
        host.requests[2].body.as_deref(),
        Some("{\"u\":\"\\ud83d\"}")
    );
    assert_eq!(host.requests[3].body.as_deref(), Some("{\"n\":1e-7}"));
    assert_eq!(host.requests[4].body.as_deref(), Some("{\"n\":0}"));
    assert_eq!(
        host.requests[5].body.as_deref(),
        Some("{\"fn\":{\"arityValue\":1}}")
    );
    assert_eq!(host.requests[6].body.as_deref(), Some("{\"n\":2}"));
    assert_eq!(host.requests[7].body, None);
    assert_eq!(host.requests[8].body, None);
    assert_eq!(host.requests[9].body.as_deref(), Some("texto"));
    assert_eq!(host.requests[10].body.as_deref(), Some("{\"n\":null}"));
}

#[test]
fn http_status_errors_are_catchable_runtime_errors() {
    #[derive(Default)]
    struct ErrorHost {
        output: Vec<String>,
        requests: usize,
    }

    impl Host for ErrorHost {
        fn write_line(&mut self, line: &str) -> io::Result<()> {
            self.output.push(line.to_owned());
            Ok(())
        }

        fn request(&mut self, _request: &HttpRequest<'_>) -> io::Result<HttpResponse> {
            self.requests += 1;
            Ok(HttpResponse {
                status: 503,
                body: "indisponível".to_owned(),
            })
        }
    }

    let source = "var req = importar(\"requisição\");\ntente { req.obter(\"https://api.example/items\"); } pegue { escreva(\"status-capturado\"); }\ntente { req.obter(\"not a url\"); } pegue { escreva(\"url-capturada\"); }";
    let mut host = ErrorHost::default();

    run_with_host(source, None, &mut host).unwrap();
    assert_eq!(host.output, ["status-capturado", "url-capturada"]);
    assert_eq!(host.requests, 1);
}
