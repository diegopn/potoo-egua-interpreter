use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, SecondsFormat, TimeZone, Timelike, Utc};
use serde_json::Value as Json;

use super::Call;
use crate::HttpRequest;
use crate::runtime::{Result, Runtime};
use crate::value::{Fields, Object, Value};

pub(super) const TIME_FUNCTIONS: &[(&str, usize)] = &[
    ("agora", 0),
    ("segundos", 0),
    ("minutos", 0),
    ("horas", 0),
    ("texto_para_data", 1),
    ("dia", 0),
    ("mes", 0),
    ("ano", 0),
    ("dia_da_semana", 0),
    ("formate_data", 2),
    ("diferença", 2),
    ("diferenca", 2),
    ("pausa", 1),
];

pub(super) const HTTP_FUNCTIONS: &[(&str, usize)] = &[
    ("obter", 1),
    ("enviar", 2),
    ("atualizar", 2),
    ("excluir", 1),
];

const WEEKDAYS: [&str; 7] = [
    "domingo",
    "segunda-feira",
    "terça-feira",
    "quarta-feira",
    "quinta-feira",
    "sexta-feira",
    "sábado",
];
const MONTHS: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];

pub(in crate::builtins) fn time(call: Call<'_, '_, '_>) -> Result<Value> {
    match call.name {
        "texto_para_data" => {
            let text = String::from_utf16_lossy(call.text(0)?);
            let bytes = text.as_bytes();
            if bytes.len() != 10
                || bytes[2] != b'/'
                || bytes[5] != b'/'
                || !(text.starts_with(|ch: char| ch.is_ascii_digit()))
                || !matches!(&bytes[6..8], b"19" | b"20")
                || bytes
                    .iter()
                    .enumerate()
                    .any(|(i, byte)| i != 2 && i != 5 && !byte.is_ascii_digit())
            {
                return Err(call.error("A data deve ser um texto no formato DD/MM/AAAA."));
            }
            let date = NaiveDate::parse_from_str(&text, "%d/%m/%Y").map_err(|_| {
                call.error(format!("Data inválida: '{text}' não existe no calendário."))
            })?;
            let utc = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"));
            let offset = call.runtime.host.local_offset(utc);
            let instant = utc - chrono::Duration::seconds(i64::from(offset.local_minus_utc()));
            Ok(Value::Date(Rc::new(instant)))
        }
        "formate_data" => {
            let Value::Date(date) = call.arg(0) else {
                return Err(call.invalid());
            };
            let template = String::from_utf16_lossy(call.text(1)?);
            let offset = call.runtime.host.local_offset(**date);
            let local = date.with_timezone(&offset);
            Ok(Value::text(
                &template
                    .replacen("DD", &format!("{:02}", local.day()), 1)
                    .replacen("MM", &format!("{:02}", local.month()), 1)
                    .replacen("AAAA", &local.year().to_string(), 1),
            ))
        }
        "diferença" | "diferenca" => {
            let (Value::Date(a), Value::Date(b)) = (call.arg(0), call.arg(1)) else {
                return Err(call.invalid());
            };
            let days = a.timestamp_millis().abs_diff(b.timestamp_millis()) / 86_400_000;
            Ok(Value::Number(days as f64))
        }
        "pausa" => {
            let millis = call.number(0)?;
            if millis < 0.0 {
                return Err(call.invalid());
            }
            if !millis.is_nan() {
                let duration = Duration::try_from_secs_f64(millis.ceil() / 1000.0)
                    .map_err(|_| call.invalid())?;
                call.runtime
                    .host
                    .wait(duration)
                    .map_err(|error| call.runtime.host_error(call.at, error))?;
            }
            Ok(Value::Undefined)
        }
        name => {
            let date = call.runtime.host.now();
            let date = DateTime::from_timestamp_millis(date.timestamp_millis()).unwrap_or(date);
            if name == "agora" {
                return Ok(Value::Date(Rc::new(date)));
            }
            let offset = call.runtime.host.local_offset(date);
            let local = date.with_timezone(&offset);
            Ok(Value::Number(match name {
                "segundos" => f64::from(local.second()),
                "minutos" => f64::from(local.minute()),
                "horas" => f64::from(local.hour()),
                "dia" => f64::from(local.day()),
                "mes" => f64::from(local.month()),
                "ano" => f64::from(local.year()),
                "dia_da_semana" => return Ok(Value::text(weekday(local))),
                _ => unreachable!(),
            }))
        }
    }
}

fn weekday(date: DateTime<FixedOffset>) -> &'static str {
    WEEKDAYS[date.weekday().num_days_from_sunday() as usize]
}

pub(crate) fn display_date(date: DateTime<FixedOffset>) -> String {
    let offset = date.offset().local_minus_utc();
    let zone = if offset == 0 {
        "Horário Universal Coordenado".to_owned()
    } else {
        format!(
            "GMT{}{:02}:{:02}",
            if offset < 0 { "-" } else { "+" },
            offset.unsigned_abs() / 3600,
            offset.unsigned_abs() / 60 % 60
        )
    };
    format!(
        "{}, {} de {} de {} às {} {zone}",
        weekday(date),
        date.day(),
        MONTHS[date.month0() as usize],
        date.year(),
        date.format("%H:%M:%S")
    )
}

pub(super) fn http(call: Call<'_, '_, '_>) -> Result<Value> {
    let url = String::from_utf16_lossy(call.text(0)?);
    let url = if url.starts_with("http://") || url.starts_with("https://") {
        url
    } else {
        format!("https://{url}")
    };
    url::Url::parse(&url).map_err(|_| call.error(format!("URL inválida: {url}")))?;
    let (method, body) = match call.name {
        "obter" => ("GET", None),
        "excluir" => ("DELETE", None),
        "enviar" | "atualizar" => {
            let value = call.arg(1);
            let body = match value {
                Value::Null | Value::Undefined => None,
                Value::Object(_) | Value::Native(_) | Value::Date(_) => Some(
                    to_json(value, &mut HashSet::new(), 0)
                        .ok_or_else(|| {
                            call.error(
                                "Não foi possível converter os dados da requisição para JSON.",
                            )
                        })?
                        .to_string(),
                ),
                _ => Some(value.js_string()),
            };
            (if call.name == "enviar" { "POST" } else { "PUT" }, body)
        }
        _ => unreachable!(),
    };
    let request = HttpRequest {
        method,
        url: &url,
        body: body.as_deref().filter(|body| !body.is_empty()),
    };
    let response = call
        .runtime
        .host
        .request(&request)
        .map_err(|error| call.runtime.host_error(call.at, error))?;
    if !(200..300).contains(&response.status) {
        return Err(call.error(format!("Erro na requisição: status {}", response.status)));
    }
    Ok(match serde_json::from_str(&response.body) {
        Ok(json) => json_value(call.runtime, json),
        Err(_) => Value::text(&response.body),
    })
}

fn to_json(
    value: &Value,
    seen: &mut HashSet<*const RefCell<Object>>,
    depth: usize,
) -> Option<Json> {
    if depth > 128 {
        return None;
    }
    Some(match value {
        Value::Null | Value::Undefined => Json::Null,
        Value::Bool(flag) => Json::Bool(*flag),
        Value::Number(number) => {
            serde_json::Number::from_f64(*number).map_or(Json::Null, Json::Number)
        }
        Value::Text(text) => Json::String(String::from_utf16_lossy(text)),
        Value::Native(_) => Json::Object(serde_json::Map::new()),
        Value::Date(date) => Json::String(date.to_rfc3339_opts(SecondsFormat::Millis, true)),
        Value::Object(object) => {
            let address = Rc::as_ptr(object);
            if !seen.insert(address) {
                return None;
            }
            let result = match &*object.borrow() {
                Object::List(items, _) => Json::Array(
                    items
                        .iter()
                        .map(|value| to_json(value, seen, depth + 1))
                        .collect::<Option<_>>()?,
                ),
                Object::Dictionary(fields)
                | Object::Module(fields)
                | Object::Class(_, fields)
                | Object::Function(_, fields)
                | Object::Instance(_, fields, _) => {
                    let mut entries = serde_json::Map::new();
                    for (key, value) in fields {
                        if !matches!(value, Value::Undefined) {
                            entries.insert(
                                String::from_utf16_lossy(key),
                                to_json(value, seen, depth + 1)?,
                            );
                        }
                    }
                    Json::Object(entries)
                }
            };
            seen.remove(&address);
            result
        }
    })
}

fn json_value(runtime: &mut Runtime<'_>, value: Json) -> Value {
    match value {
        Json::Null => Value::Null,
        Json::Bool(flag) => Value::Bool(flag),
        Json::Number(number) => Value::Number(number.as_f64().unwrap_or(f64::NAN)),
        Json::String(text) => Value::text(&text),
        Json::Array(items) => {
            let values = items
                .into_iter()
                .map(|item| json_value(runtime, item))
                .collect();
            runtime.list(values)
        }
        Json::Object(entries) => {
            let values: Fields = entries
                .into_iter()
                .map(|(key, value)| (key.encode_utf16().collect(), json_value(runtime, value)))
                .collect();
            runtime.object(Object::Dictionary(values))
        }
    }
}
