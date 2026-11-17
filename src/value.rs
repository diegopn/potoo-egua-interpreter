use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use chrono::{DateTime, Utc};

use crate::parser::Function;
use crate::runtime::Environment;

pub(super) type Fields = HashMap<Rc<[u16]>, Value>;
pub(super) type ObjectRef = Rc<RefCell<Object>>;

#[derive(Clone)]
pub(super) enum Value {
    Null,
    Undefined,
    Bool(bool),
    Number(f64),
    Text(Rc<[u16]>),
    Object(ObjectRef),
    Native(Rc<Native>),
    Date(Rc<DateTime<Utc>>),
}

pub(super) struct Native {
    pub library: &'static str,
    pub name: &'static str,
    pub arity: usize,
}

pub(super) enum Object {
    List(Vec<Value>, Fields),
    Dictionary(Fields),
    Module(Fields),
    Function(Routine, Fields),
    Class(Class, Fields),
    Instance(ObjectRef, Fields, Fields),
}

#[derive(Clone)]
pub(super) struct Routine {
    pub name: Option<String>,
    pub declaration: Rc<Function>,
    pub closure: Environment,
    pub path: Option<PathBuf>,
    pub initializer: bool,
}

pub(super) struct Class {
    pub name: String,
    pub parent: Option<ObjectRef>,
    pub methods: HashMap<String, Routine>,
}

impl Value {
    pub fn text(value: &str) -> Self {
        Self::Text(value.encode_utf16().collect())
    }

    pub fn strict_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) | (Self::Undefined, Self::Undefined) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Object(a), Self::Object(b)) => Rc::ptr_eq(a, b),
            (Self::Native(a), Self::Native(b)) => Rc::ptr_eq(a, b),
            (Self::Date(a), Self::Date(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    pub fn same_value_zero(&self, other: &Self) -> bool {
        self.strict_eq(other)
            || matches!((self, other), (Self::Number(a), Self::Number(b)) if a.is_nan() && b.is_nan())
    }

    pub fn truthy(&self) -> bool {
        !matches!(self, Self::Null | Self::Bool(false))
    }

    pub fn js_truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null | Self::Bool(false) => false,
            Self::Number(n) => *n != 0.0 && !n.is_nan(),
            Self::Text(s) => !s.is_empty(),
            _ => true,
        }
    }

    pub fn nullish(self) -> Self {
        if matches!(self, Self::Undefined) {
            Self::Null
        } else {
            self
        }
    }

    pub fn js_number(&self) -> f64 {
        match self {
            Self::Null => 0.0,
            Self::Bool(b) => f64::from(*b),
            Self::Number(n) => *n,
            Self::Date(d) => d.timestamp_millis() as f64,
            Self::Text(_) | Self::Object(_) => {
                let text = self.js_string();
                let text = text.trim();
                if text.is_empty() {
                    return 0.0;
                }
                if text.starts_with("0x") || text.starts_with("0X") {
                    return u64::from_str_radix(&text[2..], 16).map_or(f64::NAN, |n| n as f64);
                }
                if text.starts_with("0b") || text.starts_with("0B") {
                    return u64::from_str_radix(&text[2..], 2).map_or(f64::NAN, |n| n as f64);
                }
                if text.starts_with("0o") || text.starts_with("0O") {
                    return u64::from_str_radix(&text[2..], 8).map_or(f64::NAN, |n| n as f64);
                }
                match text {
                    "Infinity" | "+Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    _ if text
                        .bytes()
                        .all(|b| matches!(b, b'0'..=b'9' | b'e' | b'E' | b'+' | b'-' | b'.')) =>
                    {
                        text.parse().unwrap_or(f64::NAN)
                    }
                    _ => f64::NAN,
                }
            }
            _ => f64::NAN,
        }
    }

    pub fn js_string(&self) -> String {
        self.format(false, false, &mut HashSet::new())
    }

    pub fn output(&self) -> String {
        self.format(true, false, &mut HashSet::new())
    }

    fn format(
        &self,
        output: bool,
        nested: bool,
        seen: &mut HashSet<*const RefCell<Object>>,
    ) -> String {
        match self {
            Self::Null | Self::Undefined if output && !nested => "nulo".into(),
            Self::Bool(true) if output && !nested => "verdadeiro".into(),
            Self::Bool(false) if output && !nested => "falso".into(),
            Self::Null => "null".into(),
            Self::Undefined => "undefined".into(),
            Self::Bool(b) => b.to_string(),
            Self::Number(n) => number_string(*n),
            Self::Text(s) if output && nested => format!("{:?}", String::from_utf16_lossy(s)),
            Self::Text(s) => String::from_utf16_lossy(s),
            Self::Native(_) => "<função>".into(),
            Self::Date(d) => d.to_rfc3339(),
            Self::Object(object) => {
                let address = Rc::as_ptr(object);
                if !seen.insert(address) {
                    return if output {
                        "[Circular]".into()
                    } else {
                        String::new()
                    };
                }
                let value = match &*object.borrow() {
                    Object::List(items, _) => {
                        let parts: Vec<_> = items
                            .iter()
                            .map(|item| {
                                if !output && matches!(item, Self::Null | Self::Undefined) {
                                    String::new()
                                } else {
                                    item.format(output, true, seen)
                                }
                            })
                            .collect();
                        if output && parts.is_empty() {
                            "[]".into()
                        } else if output {
                            format!("[ {} ]", parts.join(", "))
                        } else {
                            parts.join(",")
                        }
                    }
                    Object::Dictionary(_) => "[object Object]".into(),
                    Object::Function(routine, _) => routine
                        .name
                        .as_ref()
                        .map_or_else(|| "<função>".into(), |name| format!("<função {name}>")),
                    Object::Class(class, _) => format!("<classe {}>", class.name),
                    Object::Instance(class, _, _) => match &*class.borrow() {
                        Object::Class(class, _) => format!("<Objeto {}>", class.name),
                        _ => unreachable!(),
                    },
                    Object::Module(fields) => {
                        let key: Vec<_> = "name".encode_utf16().collect();
                        fields
                            .get(key.as_slice())
                            .filter(|name| name.js_truthy())
                            .map_or_else(
                                || "<module>".into(),
                                |name| format!("<module {}>", name.format(false, false, seen)),
                            )
                    }
                };
                seen.remove(&address);
                value
            }
        }
    }
}

pub(super) fn number_string(n: f64) -> String {
    match n {
        n if n == f64::INFINITY => "Infinity".into(),
        n if n == f64::NEG_INFINITY => "-Infinity".into(),
        n if n == 0.0 => "0".into(),
        n if n.abs() >= 1e21 => format!("{n:e}").replace('e', "e+"),
        n if n.abs() < 1e-6 => format!("{n:e}"),
        n => n.to_string(),
    }
}

pub(super) fn bit_number(n: f64) -> i32 {
    n.trunc().rem_euclid(4_294_967_296.0) as u32 as i32
}
