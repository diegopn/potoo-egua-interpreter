use super::Call;
use crate::runtime::Result;
use crate::value::{Value, bit_number};

pub(super) const FUNCTIONS: &[(&str, usize)] = &[
    ("maiúsculo", 1),
    ("minúsculo", 1),
    ("contém", 2),
    ("formate", 1),
    ("dividir", 1),
    ("tamanho", 1),
    ("repita", 1),
    ("substituir", 3),
    ("substituir_tudo", 3),
    ("busca", 2),
    ("apara", 1),
    ("caractere_em", 2),
    ("código_em", 2),
    ("fatiar", 3),
    ("começa_com", 2),
    ("termina_com", 2),
    ("preencha_início", 3),
    ("preencha_fim", 3),
];

pub(super) const CONSTANTS: &[(&str, &str)] = &[
    ("letras_minúsculas", "abcdefghijklmnopqrstuvwxyz"),
    ("letras_maiúsculas", "ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
    (
        "letras",
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    ),
    ("dígitos", "0123456789"),
    ("pontuações", "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"),
];

pub(super) fn invoke(call: Call<'_, '_, '_>) -> Result<Value> {
    let value = call.text(0)?;
    Ok(match call.name {
        "maiúsculo" => Value::text(&String::from_utf16_lossy(value).to_uppercase()),
        "minúsculo" => Value::text(&String::from_utf16_lossy(value).to_lowercase()),
        "apara" => Value::Text(trim_whitespace(value).into()),
        "tamanho" => Value::Number(value.len() as f64),
        "contém" => Value::Bool(position(value, call.text(1)?).is_some()),
        "começa_com" => Value::Bool(value.starts_with(call.text(1)?)),
        "termina_com" => Value::Bool(value.ends_with(call.text(1)?)),
        "busca" => Value::Number(position(value, call.text(1)?).map_or(-1.0, |i| i as f64)),
        "formate" => {
            let marker = [u16::from(b'{'), u16::from(b'}')];
            let mut result = value.to_vec();
            for argument in &call.args[1..] {
                let replacement = match argument {
                    Value::Bool(true) => "verdadeiro".into(),
                    Value::Bool(false) => "falso".into(),
                    Value::Null => "nulo".into(),
                    Value::Object(_) | Value::Date(_) | Value::Native(_) => "<dicionário>".into(),
                    _ => argument.js_string(),
                };
                result = replace_first(
                    &result,
                    &marker,
                    &replacement.encode_utf16().collect::<Vec<_>>(),
                );
            }
            Value::Text(result.into())
        }
        "dividir" => {
            let separator = if matches!(call.arg(1), Value::Undefined) {
                &[]
            } else {
                call.text(1)?
            };
            let limit = if matches!(call.arg(2), Value::Undefined) {
                u32::MAX as usize
            } else {
                (bit_number(call.number(2)?) as u32) as usize
            };
            let pieces = split(value, separator)
                .take(limit)
                .map(|part| Value::Text(part.into()))
                .collect();
            call.runtime.list(pieces)
        }
        "repita" => {
            let count = if matches!(call.arg(1), Value::Undefined) {
                0.0
            } else {
                call.number(1)?.trunc()
            };
            if count < 0.0 || count.is_infinite() {
                return Err(call.error("Quantidade de repetições inválida."));
            }
            let count = count as usize;
            let length = value
                .len()
                .checked_mul(count)
                .ok_or_else(|| call.error("O texto resultante é grande demais."))?;
            Value::Text(pad_with(value, length, &call)?.into())
        }
        "substituir" => Value::Text(replace_first(value, call.text(1)?, call.text(2)?).into()),
        "substituir_tudo" => {
            let separator = call.text(1)?;
            let replacement = call.text(2)?;
            let mut result = Vec::new();
            for (index, part) in split(value, separator).enumerate() {
                if index != 0 {
                    result.extend_from_slice(replacement);
                }
                result.extend_from_slice(part);
            }
            Value::Text(result.into())
        }
        "caractere_em" | "código_em" => {
            let index = call.number(1)?.trunc();
            let unit = if index >= 0.0 || index.is_nan() {
                value.get(index as usize).copied()
            } else {
                None
            };
            if call.name == "código_em" {
                Value::Number(unit.map_or(f64::NAN, f64::from))
            } else {
                Value::Text(unit.into_iter().collect())
            }
        }
        "fatiar" => {
            let start = slice_index(call.number(1)?, value.len());
            let end = if matches!(call.arg(2), Value::Undefined) {
                value.len()
            } else {
                slice_index(call.arg(2).js_number(), value.len())
            };
            Value::Text(value[start..end.max(start)].into())
        }
        "preencha_início" | "preencha_fim" => {
            let length = (call.number(1)?.trunc() as usize).saturating_sub(value.len());
            let pad = if matches!(call.arg(2), Value::Undefined) {
                &[u16::from(b' ')][..]
            } else {
                call.text(2)?
            };
            let mut padding = pad_with(pad, length, &call)?;
            if call.name == "preencha_início" {
                padding.extend_from_slice(value);
                Value::Text(padding.into())
            } else {
                Value::Text(value.iter().chain(padding.iter()).copied().collect())
            }
        }
        _ => unreachable!(),
    })
}

fn position(text: &[u16], needle: &[u16]) -> Option<usize> {
    if needle.is_empty() {
        Some(0)
    } else {
        text.windows(needle.len())
            .position(|window| window == needle)
    }
}

fn split<'a>(mut text: &'a [u16], separator: &'a [u16]) -> impl Iterator<Item = &'a [u16]> {
    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        if separator.is_empty() {
            let (first, rest) = text.split_first()?;
            text = rest;
            return Some(std::slice::from_ref(first));
        }
        if let Some(index) = position(text, separator) {
            let part = &text[..index];
            text = &text[index + separator.len()..];
            Some(part)
        } else {
            done = true;
            Some(text)
        }
    })
}

fn slice_index(relative: f64, len: usize) -> usize {
    let index = relative.trunc();
    if index < 0.0 {
        (len as f64 + index) as usize
    } else {
        (index as usize).min(len)
    }
}

fn pad_with(pad: &[u16], length: usize, call: &Call<'_, '_, '_>) -> Result<Vec<u16>> {
    if pad.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(length)
        .map_err(|_| call.error("O texto resultante é grande demais."))?;
    result.extend(pad.iter().copied().cycle().take(length));
    Ok(result)
}

fn replace_first(text: &[u16], search: &[u16], replacement: &[u16]) -> Vec<u16> {
    let Some(index) = position(text, search) else {
        return text.to_vec();
    };
    let before = &text[..index];
    let after = &text[index + search.len()..];
    let mut result = before.to_vec();
    let mut symbols = replacement.iter().copied().peekable();
    while let Some(symbol) = symbols.next() {
        if symbol == u16::from(b'$') {
            let expansion = match symbols.peek().copied() {
                Some(36) => Some(&[u16::from(b'$')][..]),
                Some(38) => Some(search),
                Some(96) => Some(before),
                Some(39) => Some(after),
                _ => None,
            };
            if let Some(expansion) = expansion {
                result.extend_from_slice(expansion);
                symbols.next();
                continue;
            }
        }
        result.push(symbol);
    }
    result.extend_from_slice(after);
    result
}

fn trim_whitespace(text: &[u16]) -> &[u16] {
    let whitespace = |&unit| matches!(unit, 0x09..=0x0D | 0x20 | 0xA0 | 0x1680 | 0x2000..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000 | 0xFEFF);
    let start = text
        .iter()
        .position(|unit| !whitespace(unit))
        .unwrap_or(text.len());
    let end = text
        .iter()
        .rposition(|unit| !whitespace(unit))
        .map_or(start, |i| i + 1);
    &text[start..end]
}
