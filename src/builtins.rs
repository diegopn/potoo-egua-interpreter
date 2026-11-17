mod math;
mod system;
mod text;

use std::collections::HashMap;
use std::rc::Rc;

use crate::Location;
use crate::runtime::{Result, Runtime, Signal};
use crate::value::{Fields, Native, Object, ObjectRef, Value};

pub(super) use system::display_date;

const GLOBAL: &str = "";
const MATH: &str = "matemática";
const TEXT: &str = "textos";
const TIME: &str = "tempo";
const HTTP: &str = "requisição";

const GLOBAL_FUNCTIONS: &[&str] = &[
    "aleatório",
    "aleatório_entre",
    "inteiro",
    "para_cada",
    "mapear",
    "filtrar",
    "reduzir",
    "encontrar",
    "encontrar_último",
    "encontrar_índice",
    "encontrar_último_índice",
    "incluído",
    "algum",
    "todos",
    "ordenar",
    "real",
    "tamanho",
    "texto",
];

pub(super) fn globals() -> HashMap<String, Value> {
    GLOBAL_FUNCTIONS
        .iter()
        .map(|&name| (name.to_owned(), native(GLOBAL, name, 1)))
        .collect()
}

fn native(library: &'static str, name: &'static str, arity: usize) -> Value {
    Value::Native(Rc::new(Native {
        library,
        name,
        arity,
    }))
}

pub(super) fn module(name: &str) -> Option<Object> {
    let (name, functions) = match name {
        MATH => (MATH, math::FUNCTIONS),
        TEXT => (TEXT, text::FUNCTIONS),
        TIME => (TIME, system::TIME_FUNCTIONS),
        HTTP => (HTTP, system::HTTP_FUNCTIONS),
        _ => return None,
    };
    let mut fields: Fields = functions
        .iter()
        .map(|&(function, arity)| {
            (
                function.encode_utf16().collect(),
                native(name, function, arity),
            )
        })
        .collect();
    match name {
        MATH => {
            fields.insert(
                "pi".encode_utf16().collect(),
                Value::Number(std::f64::consts::PI),
            );
        }
        TEXT => {
            for &(key, value) in text::CONSTANTS {
                fields.insert(key.encode_utf16().collect(), Value::text(value));
            }
        }
        _ => {}
    }
    fields.insert("name".encode_utf16().collect(), Value::text(name));
    Some(Object::Module(fields))
}

pub(super) fn invoke(
    runtime: &mut Runtime<'_>,
    native: &Native,
    args: &[Value],
    at: Location,
) -> Result<Value> {
    let call = Call {
        runtime,
        at,
        args,
        name: native.name,
    };
    match native.library {
        GLOBAL => global(call),
        MATH => math::invoke(call),
        TEXT => text::invoke(call),
        TIME => system::time(call),
        HTTP => system::http(call),
        _ => unreachable!(),
    }
}

struct Call<'args, 'runtime, 'host> {
    runtime: &'runtime mut Runtime<'host>,
    at: Location,
    args: &'args [Value],
    name: &'static str,
}

impl<'args> Call<'args, '_, '_> {
    fn arg(&self, index: usize) -> &'args Value {
        self.args.get(index).unwrap_or(&Value::Undefined)
    }

    fn error(&self, message: impl Into<String>) -> Signal {
        self.runtime.error(self.at, message)
    }

    fn invalid(&self) -> Signal {
        self.error(format!("Parâmetro inválido para '{}'.", self.name))
    }

    fn number(&self, index: usize) -> Result<f64> {
        if let Value::Number(number) = self.arg(index) {
            Ok(*number)
        } else {
            Err(self.invalid())
        }
    }

    fn coerce_number(&self, index: usize) -> Result<f64> {
        let number = self.arg(index).js_number();
        if matches!(self.arg(index), Value::Null) || number.is_nan() {
            Err(self.invalid())
        } else {
            Ok(number)
        }
    }

    fn text(&self, index: usize) -> Result<&'args [u16]> {
        if let Value::Text(text) = self.arg(index) {
            Ok(text)
        } else {
            Err(self.invalid())
        }
    }

    fn poll(&mut self) -> Result<()> {
        self.runtime.poll(self.at)
    }
}

fn list_ref(value: &Value) -> Option<&ObjectRef> {
    match value {
        Value::Object(object) if matches!(&*object.borrow(), Object::List(..)) => Some(object),
        _ => None,
    }
}

fn list_items(value: &Value) -> Option<Vec<Value>> {
    let object = list_ref(value)?;
    let Object::List(items, _) = &*object.borrow() else {
        unreachable!()
    };
    Some(items.clone())
}

fn item(list: &ObjectRef, index: usize) -> Option<Value> {
    let Object::List(items, _) = &*list.borrow() else {
        unreachable!()
    };
    items.get(index).cloned()
}

fn global(mut call: Call<'_, '_, '_>) -> Result<Value> {
    match call.name {
        "aleatório" => Ok(Value::Number(call.runtime.random())),
        "aleatório_entre" => {
            if call.args.len() > 2 {
                return Err(call.error("A quantidade de argumentos máxima é 2."));
            }
            let min = call.number(0)?;
            let max = if call.args.len() == 2 {
                call.number(1)?
            } else {
                0.0
            };
            Ok(Value::Number(
                (call.runtime.random() * (max - min)).floor() + min,
            ))
        }
        "inteiro" | "real" => {
            let input = call.arg(0).js_string();
            let integer = call.name == "inteiro";
            let value = parse_numeric(&input, integer).ok_or_else(|| {
                call.error(format!("Somente números podem passar para {}.", call.name))
            })?;
            Ok(Value::Number(value))
        }
        "texto" => Ok(Value::text(&match call.arg(0) {
            Value::Null | Value::Undefined => "nulo".into(),
            Value::Bool(true) => "verdadeiro".into(),
            Value::Bool(false) => "falso".into(),
            Value::Date(_) | Value::Native(_) => "<dicionário>".into(),
            Value::Object(object) if !matches!(&*object.borrow(), Object::List(..)) => {
                "<dicionário>".into()
            }
            value => value.js_string(),
        })),
        "tamanho" => length(&call),
        "ordenar" => {
            let list = list_ref(call.arg(0)).ok_or_else(|| call.invalid())?;
            let count = match &*list.borrow() {
                Object::List(items, _) => items.len(),
                _ => unreachable!(),
            };
            loop {
                call.poll()?;
                let mut swapped = false;
                for index in 1..count {
                    let left = item(list, index - 1).unwrap_or(Value::Undefined);
                    let right = item(list, index).unwrap_or(Value::Undefined);
                    if js_greater(&left, &right) {
                        let Object::List(items, _) = &mut *list.borrow_mut() else {
                            unreachable!()
                        };
                        items.swap(index - 1, index);
                        swapped = true;
                    }
                }
                if !swapped {
                    break;
                }
            }
            Ok(call.arg(0).clone())
        }
        "incluído" => {
            let items = list_items(call.arg(0)).ok_or_else(|| call.invalid())?;
            Ok(Value::Bool(
                items.iter().any(|item| item.strict_eq(call.arg(1))),
            ))
        }
        _ => callbacks(call),
    }
}

fn length(call: &Call<'_, '_, '_>) -> Result<Value> {
    match call.arg(0) {
        Value::Text(text) => Ok(Value::Number(text.len() as f64)),
        Value::Native(native) => Ok(Value::Number(native.arity as f64)),
        Value::Null | Value::Undefined | Value::Number(_) => Err(call.invalid()),
        Value::Object(object) => match &*object.borrow() {
            Object::List(items, _) => Ok(Value::Number(items.len() as f64)),
            Object::Function(routine, _) => {
                Ok(Value::Number(routine.declaration.parameters.len() as f64))
            }
            Object::Class(..) => Ok(Value::Number(0.0)),
            Object::Instance(..) => Err(call.invalid()),
            Object::Dictionary(fields) | Object::Module(fields) => Ok(fields
                .get(&"length".encode_utf16().collect::<Vec<_>>()[..])
                .cloned()
                .unwrap_or(Value::Undefined)),
        },
        _ => Ok(Value::Undefined),
    }
}

fn callbacks(mut call: Call<'_, '_, '_>) -> Result<Value> {
    let list = list_ref(call.arg(0)).ok_or_else(|| call.invalid())?;
    let callback = call.arg(1);
    if !matches!(callback, Value::Object(object) if matches!(&*object.borrow(), Object::Function(..)))
    {
        return Err(call.error("O segundo parâmetro deve ser uma função."));
    }
    let reverse = matches!(call.name, "encontrar_último" | "encontrar_último_índice");
    let mut index = if reverse {
        let Object::List(items, _) = &*list.borrow() else {
            unreachable!()
        };
        items.len() as isize - 1
    } else {
        0
    };
    let mut accumulated = call.arg(2).clone();
    if call.name == "reduzir" && call.args.len() < 3 {
        accumulated = item(list, 0).unwrap_or(Value::Undefined);
        index = 1;
    }
    let mut values = Vec::new();
    while index >= 0
        && let Some(current) = item(list, index as usize)
    {
        call.poll()?;
        let arguments = if call.name == "reduzir" {
            vec![accumulated.clone(), current.clone()]
        } else {
            vec![current]
        };
        let result = call.runtime.call_callback(callback, &arguments, call.at)?;
        let passed = result.js_truthy();
        match call.name {
            "para_cada" => {}
            "mapear" => values.push(result),
            "filtrar" if passed => {
                values.push(item(list, index as usize).unwrap_or(Value::Undefined))
            }
            "reduzir" => accumulated = result,
            "encontrar" | "encontrar_último" if passed => {
                return Ok(item(list, index as usize).unwrap_or(Value::Undefined));
            }
            "encontrar_índice" | "encontrar_último_índice" if passed => {
                return Ok(Value::Number(index as f64));
            }
            "algum" if passed => return Ok(Value::Bool(true)),
            "todos" if !passed => return Ok(Value::Bool(false)),
            _ => {}
        }
        index += if reverse { -1 } else { 1 };
    }
    Ok(match call.name {
        "para_cada" => Value::Undefined,
        "mapear" | "filtrar" => call.runtime.list(values),
        "reduzir" => accumulated,
        "encontrar" | "encontrar_último" => Value::Null,
        "encontrar_índice" | "encontrar_último_índice" => Value::Number(-1.0),
        "algum" => Value::Bool(false),
        "todos" => Value::Bool(true),
        _ => unreachable!(),
    })
}

fn parse_numeric(text: &str, integer: bool) -> Option<f64> {
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|ch| ch.is_ascii_digit());
    let number = if let Some((integral, fraction)) = text.split_once('.') {
        if !digits(integral) || !digits(fraction) {
            return None;
        }
        if integer { integral } else { text }
    } else {
        if !digits(text.strip_prefix('-').unwrap_or(text)) {
            return None;
        }
        text
    };
    number.parse().ok()
}

fn js_greater(left: &Value, right: &Value) -> bool {
    let primitive = |value: &Value| match value {
        Value::Object(_) | Value::Native(_) => Value::text(&value.js_string()),
        other => other.clone(),
    };
    let left = primitive(left);
    let right = primitive(right);
    if let (Value::Text(left), Value::Text(right)) = (&left, &right) {
        left > right
    } else {
        left.js_number() > right.js_number()
    }
}

fn js_add(left: &Value, right: &Value) -> Value {
    if matches!(left, Value::Text(_) | Value::Object(_) | Value::Native(_))
        || matches!(right, Value::Text(_) | Value::Object(_) | Value::Native(_))
    {
        Value::text(&format!("{}{}", left.js_string(), right.js_string()))
    } else {
        Value::Number(left.js_number() + right.js_number())
    }
}
