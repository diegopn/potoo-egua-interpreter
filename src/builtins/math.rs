use super::{Call, js_add, list_items};
use crate::runtime::{Result, pow};
use crate::value::{Value, bit_number};

pub(super) const FUNCTIONS: &[(&str, usize)] = &[
    ("graus", 1),
    ("mediana", 1),
    ("moda", 1),
    ("radiano", 1),
    ("gerar_pontos_abscissa", 3),
    ("fun1R", 2),
    ("pontos_espaçados", 3),
    ("fun2R", 3),
    ("aprox", 2),
    ("número_ocorrências", 2),
    ("max", 1),
    ("min", 1),
    ("média", 0),
    ("sen", 1),
    ("cos", 1),
    ("tan", 1),
    ("arcos", 1),
    ("arc_sen", 1),
    ("arc_tan", 1),
    ("exp", 1),
    ("log", 1),
    ("potência", 2),
    ("raizq", 1),
    ("velocidade_média", 2),
    ("delta_s", 2),
    ("delta_t", 2),
    ("aceleração", 4),
    ("mrufh", 3),
    ("mruv", 3),
    ("min_aprox", 1),
    ("arredondar", 1),
    ("teto", 1),
    ("absoluto", 1),
    ("sinal", 1),
    ("raiz_cubica", 1),
    ("log_base2", 1),
    ("log_base10", 1),
    ("sen_hiperbolico", 1),
    ("cos_hiperbolico", 1),
    ("tan_hiperbolico", 1),
    ("fatorial", 1),
    ("eh_par", 1),
    ("eh_impar", 1),
    ("somatorio", 1),
    ("produto", 1),
];

pub(super) fn invoke(call: Call<'_, '_, '_>) -> Result<Value> {
    if let Some(operation) = unary_operation(call.name) {
        return Ok(Value::Number(operation(call.coerce_number(0)?)));
    }
    let number = match call.name {
        "potência" => pow(call.number(0)?, call.number(1)?),
        "min_aprox" => call.number(0)?.floor(),
        "fun1R" => -call.coerce_number(1)? / call.coerce_number(0)?,
        "fun2R" => {
            let a = call.coerce_number(0)?;
            if matches!(call.arg(0), Value::Number(n) if *n == 0.0) {
                return Err(call.invalid());
            }
            let b = call.arg(1).js_number();
            let c = call.arg(2).js_number();
            let delta = b * b - 4.0 * a * c;
            if delta < 0.0 {
                return Err(call.error("Não há raízes reais (delta negativo)."));
            }
            return Ok(call.runtime.list(vec![
                Value::Number((-b + delta.sqrt()) / (2.0 * a)),
                Value::Number((-b - delta.sqrt()) / (2.0 * a)),
            ]));
        }
        "velocidade_média" => call.coerce_number(0)? / call.coerce_number(1)?,
        "delta_s" | "delta_t" => call.coerce_number(1)? - call.coerce_number(0)?,
        "aceleração" => {
            (call.number(0)? - call.number(1)?) / (call.number(2)? - call.number(3)?)
        }
        "eh_par" => return Ok(Value::Bool(call.coerce_number(0)? % 2.0 == 0.0)),
        "eh_impar" => return Ok(Value::Bool(call.coerce_number(0)? % 2.0 != 0.0)),
        "fatorial" => {
            let n = call.number(0)?;
            if n < 0.0 || !n.is_finite() || n.fract() != 0.0 {
                return Err(call.invalid());
            }
            if n > 170.0 {
                f64::INFINITY
            } else {
                (2..=n as u32).map(f64::from).product()
            }
        }
        "gerar_pontos_abscissa" | "pontos_espaçados" | "mrufh" | "mruv" => return sequence(call),
        "aprox" => return approximate(call),
        _ => return statistics(call),
    };
    Ok(Value::Number(number))
}

fn unary_operation(name: &str) -> Option<fn(f64) -> f64> {
    Some(match name {
        "graus" => |x| x * (180.0 / std::f64::consts::PI),
        "radiano" => |x| x * (std::f64::consts::PI / 180.0),
        "sen" => f64::sin,
        "cos" => f64::cos,
        "tan" => f64::tan,
        "arcos" => f64::acos,
        "arc_sen" => f64::asin,
        "arc_tan" => f64::atan,
        "exp" => f64::exp,
        "log" => f64::ln,
        "raizq" => f64::sqrt,
        "arredondar" => |x| {
            if !x.is_finite() || x == 0.0 {
                x
            } else if (-0.5..0.0).contains(&x) {
                -0.0
            } else if x - x.floor() >= 0.5 {
                x.ceil()
            } else {
                x.floor()
            }
        },
        "teto" => f64::ceil,
        "absoluto" => f64::abs,
        "sinal" => |x| if x == 0.0 { x } else { x.signum() },
        "raiz_cubica" => f64::cbrt,
        "log_base2" => f64::log2,
        "log_base10" => f64::log10,
        "sen_hiperbolico" => f64::sinh,
        "cos_hiperbolico" => f64::cosh,
        "tan_hiperbolico" => f64::tanh,
        _ => return None,
    })
}

fn statistics(mut call: Call<'_, '_, '_>) -> Result<Value> {
    let mut items = list_items(call.arg(0)).ok_or_else(|| call.invalid())?;
    let number = match call.name {
        "número_ocorrências" => items
            .iter()
            .filter(|item| item.strict_eq(call.arg(1)))
            .count() as f64,
        "mediana" | "moda" | "max" | "min" => {
            if items.iter().any(|value| value.js_number().is_nan()) {
                return Err(call.invalid());
            }
            match call.name {
                "mediana" => {
                    if items.is_empty() {
                        return Err(call.invalid());
                    }
                    items.sort_by(|left, right| {
                        left.js_number()
                            .partial_cmp(&right.js_number())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let middle = items.len() / 2;
                    if items.len() % 2 == 1 {
                        return Ok(items[middle].clone());
                    }
                    js_add(&items[middle - 1], &items[middle]).js_number() / 2.0
                }
                "max" => items
                    .iter()
                    .map(Value::js_number)
                    .fold(f64::NEG_INFINITY, f64::max),
                "min" => items
                    .iter()
                    .map(Value::js_number)
                    .fold(f64::INFINITY, f64::min),
                "moda" => {
                    let mut counts = Vec::<(String, usize)>::new();
                    let mut indices = std::collections::HashMap::new();
                    for item in items {
                        call.poll()?;
                        let key = item.js_string();
                        let index = *indices.entry(key.clone()).or_insert(counts.len());
                        if let Some(entry) = counts.get_mut(index) {
                            entry.1 += 1;
                        } else {
                            counts.push((key, 1));
                        }
                    }
                    let highest = counts.iter().map(|(_, count)| *count).max().unwrap_or(0);
                    counts.sort_by_key(|(key, _)| {
                        key.parse::<u32>()
                            .ok()
                            .filter(|&n| n < u32::MAX && n.to_string() == *key)
                            .unwrap_or(u32::MAX)
                    });
                    let modes = counts
                        .into_iter()
                        .filter(|(_, count)| *count == highest)
                        .map(|(key, _)| Value::Number(Value::text(&key).js_number()))
                        .collect();
                    return Ok(call.runtime.list(modes));
                }
                _ => unreachable!(),
            }
        }
        "média" | "somatorio" | "produto" => {
            if items.is_empty()
                || (call.name == "média" && call.args.len() != 1)
                || items.iter().any(|item| !matches!(item, Value::Number(_)))
            {
                return Err(call.invalid());
            }
            let numbers = items.iter().map(Value::js_number);
            match call.name {
                "média" => numbers.sum::<f64>() / items.len() as f64,
                "somatorio" => numbers.sum(),
                "produto" => numbers.product(),
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    };
    Ok(Value::Number(number))
}

fn sequence(mut call: Call<'_, '_, '_>) -> Result<Value> {
    let (initial, delta, count) = match call.name {
        "gerar_pontos_abscissa" => {
            let delta = call.number(0)?;
            let center = call.number(1)?;
            if !delta.is_finite()
                || !center.is_finite()
                || delta.fract() != 0.0
                || center.fract() != 0.0
            {
                return Err(call.invalid());
            }
            let count = if call.arg(2).js_truthy() {
                call.arg(2).js_number()
            } else {
                7.0
            };
            (
                Value::Number(center - f64::from(bit_number(count / 2.0)) * delta),
                delta,
                count.ceil(),
            )
        }
        "pontos_espaçados" => {
            let start = call.coerce_number(0)?;
            let end = call.coerce_number(1)?;
            let count = call.coerce_number(2)?;
            (
                call.arg(0).clone(),
                (end - start) / (count - 1.0),
                count.ceil(),
            )
        }
        "mrufh" => {
            call.coerce_number(0)?;
            (
                call.arg(0).clone(),
                call.coerce_number(1)?,
                call.coerce_number(2)?.floor() + 1.0,
            )
        }
        "mruv" => {
            call.coerce_number(0)?;
            (
                call.arg(0).clone(),
                call.coerce_number(2)?,
                call.coerce_number(1)?.ceil(),
            )
        }
        _ => unreachable!(),
    };
    if count > f64::from(u32::MAX) {
        return Err(call.error("Quantidade de valores excede o tamanho de um vetor."));
    }
    let mut values = Vec::new();
    for index in 0..count as usize {
        call.poll()?;
        values
            .try_reserve(1)
            .map_err(|_| call.error("Não foi possível expandir o vetor."))?;
        values.push(js_add(&initial, &Value::Number(delta * index as f64)));
    }
    if call.name == "mrufh" {
        let displayed = values
            .iter()
            .map(Value::js_string)
            .collect::<Vec<_>>()
            .join(",");
        values = vec![Value::text(&format!(
            "Função: {}+({})*t\nPosições: {displayed}",
            initial.js_string(),
            call.arg(1).js_string()
        ))];
    }
    Ok(call.runtime.list(values))
}

fn approximate(mut call: Call<'_, '_, '_>) -> Result<Value> {
    let precision = match call.arg(1) {
        Value::Null | Value::Undefined => 2,
        Value::Number(n) if (0.0..=100.0).contains(&n.trunc()) || n.is_nan() => *n as usize,
        _ => return Err(call.invalid()),
    };
    let rounded = |value: &Value| -> Option<Value> {
        let Value::Number(n) = value else { return None };
        let number = round_fixed(*n, precision);
        Some(Value::Number(number))
    };
    if let Some(value) = rounded(call.arg(0)) {
        return Ok(value);
    }
    let items = list_items(call.arg(0)).ok_or_else(|| call.invalid())?;
    let mut result = Vec::new();
    if items.first().and_then(list_items).is_some() {
        for row in items {
            call.poll()?;
            let row = list_items(&row).ok_or_else(|| call.invalid())?;
            let values = row
                .iter()
                .map(|value| rounded(value).ok_or_else(|| call.invalid()))
                .collect::<Result<Vec<_>>>()?;
            result.push(call.runtime.list(values));
        }
    } else {
        result = items
            .iter()
            .map(|value| rounded(value).ok_or_else(|| call.invalid()))
            .collect::<Result<Vec<_>>>()?;
    }
    Ok(call.runtime.list(result))
}

fn round_fixed(number: f64, precision: usize) -> f64 {
    if !number.is_finite() || number.abs() >= 1e21 {
        return number;
    }
    let mut decimal = format!("{number:.precision$}").into_bytes();
    // Scaling by 2^precision preserves the exact binary tie. The factor
    // 5^precision is 1 modulo 4, so the floor's parity stays the same.
    let scaled = number.abs() * 2.0_f64.powi(precision as i32);
    if scaled.fract() == 0.5 && scaled.floor() % 2.0 == 0.0 {
        let digit = decimal
            .iter_mut()
            .rev()
            .find(|byte| byte.is_ascii_digit())
            .expect("formatted number");
        *digit += 1;
    }
    std::str::from_utf8(&decimal)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(number)
}
