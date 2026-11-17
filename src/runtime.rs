use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use crate::builtins;
use crate::lexer::{Kind, Token};
use crate::parser::{Expr, Statement, Stmt};
use crate::value::{Class, Fields, Object, ObjectRef, Routine, Value, bit_number};
use crate::{Diagnostic, Host, Location, ParsedProgram, Phase, Step};

pub(super) type Environment = Rc<RefCell<Scope>>;

pub(super) struct Scope {
    parent: Option<Environment>,
    values: HashMap<String, Value>,
}

pub(super) enum Signal {
    Error(Diagnostic),
    Return(Value, Location),
    Break(Location),
    Continue(Location),
}

pub(super) type Result<T> = std::result::Result<T, Signal>;

struct Registry<T>(Vec<Weak<RefCell<T>>>);

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> Registry<T> {
    fn insert(&mut self, value: T) -> Rc<RefCell<T>> {
        if self.0.len() == self.0.capacity() {
            self.0.retain(|entry| entry.strong_count() != 0);
        }
        let value = Rc::new(RefCell::new(value));
        self.0.push(Rc::downgrade(&value));
        value
    }
}

#[derive(Default)]
pub(super) struct State {
    globals: Option<Environment>,
    environments: Registry<Scope>,
    objects: Registry<Object>,
    random: Option<u64>,
}

impl Drop for State {
    fn drop(&mut self) {
        // Environments and mutable values can refer back to themselves.
        for scope in self.environments.0.iter().filter_map(Weak::upgrade) {
            scope.borrow_mut().values.clear();
        }
        for object in self.objects.0.iter().filter_map(Weak::upgrade) {
            *object.borrow_mut() = Object::Dictionary(Fields::new());
        }
    }
}

pub(super) struct Runtime<'a> {
    state: &'a mut State,
    pub host: &'a mut dyn Host,
    globals: Environment,
    current: Environment,
    module_path: Option<PathBuf>,
    path: Option<PathBuf>,
    call_stack: Vec<Option<String>>,
    imports: usize,
}

impl State {
    pub fn execute(
        &mut self,
        statements: &[Stmt],
        path: Option<&Path>,
        host: &mut dyn Host,
    ) -> std::result::Result<(), Diagnostic> {
        let current = if let Some(globals) = &self.globals {
            globals.clone()
        } else {
            let globals = self.global_scope();
            self.globals = Some(globals.clone());
            globals
        };
        let mut runtime = Runtime {
            state: self,
            host,
            globals: current.clone(),
            current,
            module_path: None,
            path: path.map(Path::to_owned),
            call_stack: Vec::new(),
            imports: 0,
        };
        runtime.statements(statements).map_err(|signal| {
            let mut diagnostic = match signal {
                Signal::Error(error) => error,
                Signal::Return(_, at) | Signal::Break(at) | Signal::Continue(at) => at.error(
                    Phase::Runtime,
                    "Comando de controle escapou do escopo de execução.",
                ),
            };
            if diagnostic.path.is_none() {
                diagnostic.path = runtime.path.clone();
            }
            diagnostic
        })
    }

    fn global_scope(&mut self) -> Environment {
        let mut values = builtins::globals();
        values.insert(
            "exports".into(),
            Value::Object(self.objects.insert(Object::Dictionary(Fields::new()))),
        );
        self.environments.insert(Scope {
            parent: None,
            values,
        })
    }
}

impl Runtime<'_> {
    pub fn error(&self, at: Location, message: impl Into<String>) -> Signal {
        let mut error = at.error(Phase::Runtime, message);
        error.path = self.path.clone();
        Signal::Error(error)
    }

    fn cancelled(&self, at: Location) -> Signal {
        let mut error = at.error(Phase::Cancelled, "A execução foi cancelada.");
        error.path = self.path.clone();
        Signal::Error(error)
    }

    pub fn host_error(&self, at: Location, error: io::Error) -> Signal {
        if error.kind() == io::ErrorKind::Interrupted {
            self.cancelled(at)
        } else {
            self.error(at, error.to_string())
        }
    }

    pub fn poll(&mut self, at: Location) -> Result<()> {
        if self.host.cancelled() {
            return Err(self.cancelled(at));
        }
        Ok(())
    }

    pub fn object(&mut self, object: Object) -> Value {
        Value::Object(self.state.objects.insert(object))
    }

    pub fn list(&mut self, values: Vec<Value>) -> Value {
        self.object(Object::List(values, Fields::new()))
    }

    fn nested(&mut self, parent: Environment) -> Environment {
        self.state.environments.insert(Scope {
            parent: Some(parent),
            values: HashMap::new(),
        })
    }

    fn define(&mut self, name: String, value: Value) {
        self.current.borrow_mut().values.insert(name, value);
    }

    fn statements(&mut self, statements: &[Stmt]) -> Result<()> {
        for statement in statements {
            self.statement(statement)?;
        }
        Ok(())
    }

    fn in_scope(&mut self, statements: &[Stmt], environment: Environment) -> Result<()> {
        let previous = std::mem::replace(&mut self.current, environment);
        let result = self.statements(statements);
        self.current = previous;
        result
    }

    fn block(&mut self, statements: &[Stmt]) -> Result<()> {
        let environment = self.nested(self.current.clone());
        self.in_scope(statements, environment)
    }

    fn step(&mut self, at: Location) -> Result<()> {
        self.poll(at)?;
        let inspect = |name: &str| find(&self.current, name).map(|value| value.output());
        let step = Step {
            path: self.path.as_deref(),
            line: at.line,
            column: at.column,
            routine: self.call_stack.last().and_then(|name| name.as_deref()),
            call_depth: self.call_stack.len(),
            inspect: &inspect,
        };
        if self.host.on_step(&step).is_break() {
            return Err(self.cancelled(at));
        }
        Ok(())
    }

    fn statement(&mut self, statement: &Stmt) -> Result<()> {
        let at = statement.location;
        self.step(at)?;
        match &statement.kind {
            Statement::Var(name, initializer) => {
                let value = self.optional_value(initializer.as_ref())?;
                self.define(name.text.clone(), value);
            }
            Statement::Print(expression) => {
                let value = self.evaluate(expression)?;
                let text = match value {
                    Value::Date(date) => {
                        let offset = self.host.local_offset(*date);
                        builtins::display_date(date.with_timezone(&offset))
                    }
                    _ => value.output(),
                };
                self.host
                    .write_line(&text)
                    .map_err(|error| self.host_error(at, error))?;
            }
            Statement::Evaluate(expression) => {
                self.evaluate(expression)?;
            }
            Statement::Block(statements) => self.block(statements)?,
            Statement::If {
                branches,
                otherwise,
            } => {
                for (condition, branch) in branches {
                    if self.evaluate(condition)?.truthy() {
                        return self.statement(branch);
                    }
                }
                if let Some(otherwise) = otherwise {
                    self.statement(otherwise)?;
                }
            }
            Statement::Loop {
                initializer,
                condition,
                step,
                body,
                post_condition,
            } => {
                if let Some(initializer) = initializer {
                    self.statement(initializer)?;
                }
                let mut first = true;
                loop {
                    self.poll(at)?;
                    if !(*post_condition && first)
                        && let Some(condition) = condition
                        && !self.evaluate(condition)?.truthy()
                    {
                        break;
                    }
                    first = false;
                    match self.statement(body) {
                        Ok(()) | Err(Signal::Continue(_)) => {}
                        Err(Signal::Break(_)) => break,
                        error => error?,
                    }
                    if let Some(step) = step {
                        self.evaluate(step)?;
                    }
                }
            }
            Statement::Switch {
                subject,
                cases,
                otherwise,
            } => {
                let value = self.evaluate(subject)?;
                let mut matched = false;
                for (labels, body) in cases {
                    for label in labels {
                        if value.strict_eq(&self.evaluate(label)?) {
                            matched = true;
                            match self.statements(body) {
                                Ok(()) | Err(Signal::Continue(_)) => {}
                                Err(Signal::Break(_)) => return Ok(()),
                                error => error?,
                            }
                        }
                    }
                }
                if !matched && let Some(otherwise) = otherwise {
                    match self.statements(otherwise) {
                        Err(Signal::Break(_)) => {}
                        result => result?,
                    }
                }
            }
            Statement::Try {
                body,
                catch,
                otherwise,
                finally,
            } => {
                let result = match self.block(body) {
                    Ok(()) => otherwise
                        .as_ref()
                        .map_or(Ok(()), |branch| self.block(branch)),
                    Err(Signal::Error(error)) if error.phase == Phase::Cancelled => {
                        return Err(Signal::Error(error));
                    }
                    Err(_) => catch.as_ref().map_or(Ok(()), |branch| self.block(branch)),
                };
                if !matches!(&result, Err(Signal::Error(error)) if error.phase == Phase::Cancelled)
                    && let Some(finally) = finally
                {
                    self.block(finally)?;
                }
                result?;
            }
            Statement::Function(name, declaration) => {
                let function = self.object(Object::Function(
                    Routine {
                        name: Some(name.text.clone()),
                        declaration: declaration.clone(),
                        closure: self.current.clone(),
                        path: self.path.clone(),
                        initializer: false,
                    },
                    Fields::new(),
                ));
                self.define(name.text.clone(), function);
            }
            Statement::Class(name, parent, methods) => {
                let parent = match parent {
                    Some(expression) => match self.evaluate(expression)? {
                        Value::Object(object) if matches!(&*object.borrow(), Object::Class(..)) => {
                            Some(object)
                        }
                        _ => {
                            return Err(
                                self.error(name.location, "Superclasse precisa ser uma classe.")
                            );
                        }
                    },
                    None => None,
                };
                self.define(name.text.clone(), Value::Null);
                let closure = if let Some(parent) = &parent {
                    let scope = self.nested(self.current.clone());
                    scope
                        .borrow_mut()
                        .values
                        .insert("super".into(), Value::Object(parent.clone()));
                    scope
                } else {
                    self.current.clone()
                };
                let methods = methods
                    .iter()
                    .map(|(name, declaration)| {
                        (
                            name.text.clone(),
                            Routine {
                                name: Some(name.text.clone()),
                                declaration: declaration.clone(),
                                closure: closure.clone(),
                                path: self.path.clone(),
                                initializer: name.text == "construtor",
                            },
                        )
                    })
                    .collect();
                let class = self.object(Object::Class(
                    Class {
                        name: name.text.clone(),
                        parent,
                        methods,
                    },
                    Fields::new(),
                ));
                self.define(name.text.clone(), class);
            }
            Statement::Return(expression) => {
                return Err(Signal::Return(
                    self.optional_value(expression.as_ref())?,
                    at,
                ));
            }
            Statement::Break => return Err(Signal::Break(at)),
            Statement::Continue => return Err(Signal::Continue(at)),
        }
        Ok(())
    }

    fn optional_value(&mut self, expression: Option<&Expr>) -> Result<Value> {
        expression.map_or(Ok(Value::Null), |expression| self.evaluate(expression))
    }

    fn resolved_depth(&self, depth: Option<usize>) -> Option<usize> {
        // An imported function keeps its closure. Its bindings belong to the
        // interpreter that resolved its declaration.
        depth.filter(|_| Rc::ptr_eq(&self.globals, &global(&self.current)))
    }

    fn lookup(&self, name: &Token, depth: Option<usize>) -> Result<Value> {
        if let Some(depth) = self.resolved_depth(depth) {
            return Ok(ancestor(&self.current, depth)
                .and_then(|scope| scope.borrow().values.get(&name.text).cloned())
                .unwrap_or(Value::Undefined));
        }
        find(&self.globals, &name.text).ok_or_else(|| {
            self.error(
                name.location,
                format!("Variável não definida '{}'.", name.text),
            )
        })
    }

    fn evaluate(&mut self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::Literal(value) => Ok(value.clone()),
            Expr::Name(name, depth) => self.lookup(name, *depth),
            Expr::Super(keyword, method, depth) => {
                let Value::Object(class) = self.lookup(keyword, *depth)? else {
                    return Err(self.error(keyword.location, "Superclasse não definida."));
                };
                let receiver = self
                    .resolved_depth(*depth)
                    .and_then(|n| n.checked_sub(1))
                    .and_then(|n| ancestor(&self.current, n))
                    .and_then(|scope| scope.borrow().values.get("isto").cloned())
                    .unwrap_or(Value::Undefined);
                let function = find_method(&class, &method.text)
                    .ok_or_else(|| self.error(method.location, "Método chamado indefinido."))?;
                Ok(self.bind(function, receiver))
            }
            Expr::Assign(name, depth, expression) => {
                let value = self.evaluate(expression)?;
                let scope = if let Some(depth) = self.resolved_depth(*depth) {
                    ancestor(&self.current, depth)
                } else {
                    find_scope(&self.current, &name.text)
                };
                let scope = scope.ok_or_else(|| {
                    self.error(
                        name.location,
                        format!("Variável não definida '{}'.", name.text),
                    )
                })?;
                scope
                    .borrow_mut()
                    .values
                    .insert(name.text.clone(), value.clone());
                Ok(value)
            }
            Expr::Group(expression) => self.evaluate(expression),
            Expr::Unary(operator, expression) => {
                let right = self.evaluate(expression)?;
                match operator.kind {
                    Kind::Bang => Ok(Value::Bool(!right.truthy())),
                    Kind::Minus => match right {
                        Value::Number(number) => Ok(Value::Number(-number)),
                        _ => Err(self.error(operator.location, "Operador precisa ser um número.")),
                    },
                    Kind::BitNot => Ok(Value::Number(f64::from(!bit_number(right.js_number())))),
                    _ => unreachable!(),
                }
            }
            Expr::Binary(left, operator, right) => {
                let left = self.evaluate(left)?;
                match operator.kind {
                    Kind::And if !left.truthy() => Ok(left),
                    Kind::Or if left.truthy() => Ok(left),
                    Kind::And | Kind::Or => self.evaluate(right),
                    _ => {
                        let right = self.evaluate(right)?;
                        self.binary(left, operator, right)
                    }
                }
            }
            Expr::List(expressions) => {
                let values = self.arguments(expressions)?;
                Ok(self.list(values))
            }
            Expr::Dictionary(entries) => {
                let mut values = Fields::new();
                for (key, expression) in entries {
                    let key = key_string(&self.evaluate(key)?);
                    values.insert(key, self.evaluate(expression)?);
                }
                Ok(self.object(Object::Dictionary(values)))
            }
            Expr::Function(declaration) => Ok(self.object(Object::Function(
                Routine {
                    name: None,
                    declaration: declaration.clone(),
                    closure: self.current.clone(),
                    path: self.path.clone(),
                    initializer: false,
                },
                Fields::new(),
            ))),
            Expr::Call(callee, at, arguments) => {
                let callee = self.evaluate(callee)?;
                let args = self.arguments(arguments)?;
                self.call(callee, args, *at)
            }
            Expr::Get(object, name) => {
                let object = self.evaluate(object)?;
                self.get(object, name)
            }
            Expr::Set(object, name, expr) => {
                let Value::Object(object) = self.evaluate(object)? else {
                    return Err(self.error(
                        name.location,
                        "Somente instâncias e dicionários podem possuir campos.",
                    ));
                };
                if !matches!(
                    &*object.borrow(),
                    Object::Instance(..) | Object::Dictionary(..)
                ) {
                    return Err(self.error(
                        name.location,
                        "Somente instâncias e dicionários podem possuir campos.",
                    ));
                }
                let value = self.evaluate(expr)?;
                let key = name.text.encode_utf16().collect();
                match &mut *object.borrow_mut() {
                    Object::Dictionary(fields) => {
                        fields.insert(key, value);
                        Ok(Value::Undefined)
                    }
                    Object::Instance(_, fields, _) => {
                        fields.insert(key, value.clone());
                        Ok(value)
                    }
                    _ => unreachable!(),
                }
            }
            Expr::Index(object, at, index) => {
                let object = self.evaluate(object)?;
                let index = self.evaluate(index)?;
                self.index(object, index, *at)
            }
            Expr::SetIndex(object, at, index, expr) => {
                let object = self.evaluate(object)?;
                let index = self.evaluate(index)?;
                let value = self.evaluate(expr)?;
                self.set_index(object, index, value, *at)?;
                Ok(Value::Undefined)
            }
            Expr::Import(path, at) => {
                let path = self.evaluate(path)?;
                self.import(path, *at)
            }
        }
    }

    fn arguments(&mut self, expressions: &[Expr]) -> Result<Vec<Value>> {
        expressions
            .iter()
            .map(|expression| self.evaluate(expression))
            .collect()
    }

    fn binary(&self, left: Value, operator: &Token, right: Value) -> Result<Value> {
        use Kind::*;
        match operator.kind {
            Equal => return Ok(Value::Bool(left.strict_eq(&right))),
            NotEqual => return Ok(Value::Bool(!left.strict_eq(&right))),
            In => {
                let present = match &right {
                    Value::Text(haystack) => contains_text(haystack, &key_string(&left)),
                    Value::Object(object) => match &*object.borrow() {
                        Object::List(items, _) => {
                            items.iter().any(|item| item.same_value_zero(&left))
                        }
                        Object::Dictionary(values) => values.contains_key(&key_string(&left)),
                        _ => {
                            return Err(
                                self.error(operator.location, "Tipo de chamada inválida com 'em'.")
                            );
                        }
                    },
                    _ => {
                        return Err(
                            self.error(operator.location, "Tipo de chamada inválida com 'em'.")
                        );
                    }
                };
                return Ok(Value::Bool(present));
            }
            Plus if let (Value::Text(a), Value::Text(b)) = (&left, &right) => {
                return Ok(Value::Text(a.iter().chain(b.iter()).copied().collect()));
            }
            _ => {}
        }

        let (Value::Number(left), Value::Number(right)) = (left, right) else {
            let message = if operator.kind == Plus {
                "Operadores precisam ser dois números ou duas strings."
            } else {
                "Operadores precisam ser números."
            };
            return Err(self.error(operator.location, message));
        };

        Ok(match operator.kind {
            Plus => Value::Number(left + right),
            Minus => Value::Number(left - right),
            Star => Value::Number(left * right),
            Slash => Value::Number(left / right),
            Percent => Value::Number(left % right),
            Power => Value::Number(pow(left, right)),
            Less => Value::Bool(left < right),
            LessEqual => Value::Bool(left <= right),
            Greater => Value::Bool(left > right),
            GreaterEqual => Value::Bool(left >= right),
            BitAnd => Value::Number(f64::from(bit_number(left) & bit_number(right))),
            BitOr => Value::Number(f64::from(bit_number(left) | bit_number(right))),
            BitXor => Value::Number(f64::from(bit_number(left) ^ bit_number(right))),
            ShiftLeft => Value::Number(f64::from(
                bit_number(left).wrapping_shl(bit_number(right) as u32),
            )),
            ShiftRight => Value::Number(f64::from(
                bit_number(left).wrapping_shr(bit_number(right) as u32),
            )),
            _ => unreachable!(),
        })
    }

    fn bind(&mut self, mut routine: Routine, receiver: Value) -> Value {
        let scope = self.nested(routine.closure);
        scope.borrow_mut().values.insert("isto".into(), receiver);
        routine.closure = scope;
        self.object(Object::Function(routine, Fields::new()))
    }

    fn get(&mut self, value: Value, name: &Token) -> Result<Value> {
        let Value::Object(object) = value else {
            return Err(self.error(
                name.location,
                "Você só pode acessar métodos do objeto e dicionários.",
            ));
        };
        let key: Vec<u16> = name.text.encode_utf16().collect();
        let class = match &*object.borrow() {
            Object::Instance(class, fields, _) => {
                if let Some(value) = fields.get(key.as_slice()) {
                    return Ok(value.clone().nullish());
                }
                class.clone()
            }
            Object::Dictionary(fields) | Object::Module(fields) => {
                return Ok(fields
                    .get(key.as_slice())
                    .cloned()
                    .unwrap_or(Value::Null)
                    .nullish());
            }
            _ => {
                return Err(self.error(
                    name.location,
                    "Você só pode acessar métodos do objeto e dicionários.",
                ));
            }
        };
        let method = find_method(&class, &name.text).ok_or_else(|| {
            self.error(
                name.location,
                format!("Método {} não foi definido.", name.text),
            )
        })?;
        Ok(self.bind(method, Value::Object(object)))
    }

    fn index(&self, value: Value, index: Value, at: Location) -> Result<Value> {
        let value = match value {
            Value::Text(text) => {
                let index = self.integer_index(&index, text.len(), at)?;
                let unit = (index >= 0.0).then(|| text.get(index as usize)).flatten();
                return Ok(Value::Text(unit.copied().into_iter().collect()));
            }
            Value::Object(object) => match &*object.borrow() {
                Object::List(items, properties) => {
                    let numeric = self.integer_index(&index, items.len(), at)?;
                    let value = if numeric < 0.0 {
                        properties.get(&key_string(&index))
                    } else {
                        items.get(numeric as usize)
                    };
                    return Ok(value.cloned().unwrap_or(Value::Undefined));
                }
                Object::Dictionary(fields)
                | Object::Module(fields)
                | Object::Function(_, fields)
                | Object::Class(_, fields)
                | Object::Instance(_, _, fields) => fields.get(&key_string(&index)).cloned(),
            },
            _ => return Err(self.error(at, "O valor não admite indexação.")),
        };
        Ok(value.filter(Value::js_truthy).unwrap_or(Value::Null))
    }

    fn integer_index(&self, index: &Value, len: usize, at: Location) -> Result<f64> {
        let Value::Number(n) = index else {
            return Err(self.error(
                at,
                "Somente inteiros podem ser usados para indexar um vetor.",
            ));
        };
        if !n.is_finite() || n.fract() != 0.0 {
            return Err(self.error(
                at,
                "Somente inteiros podem ser usados para indexar um vetor.",
            ));
        }
        let i = wrap_index(*n, len);
        if i >= len as f64 {
            return Err(self.error(at, "Índice fora do intervalo."));
        }
        Ok(i)
    }

    fn set_index(&mut self, value: Value, index: Value, new: Value, at: Location) -> Result<()> {
        let Value::Object(object) = value else {
            return Err(self.error(at, "O valor não admite sobrescrita por índice."));
        };
        let key = key_string(&index);
        if "length".encode_utf16().eq(key.iter().copied())
            && matches!(&*object.borrow(), Object::List(..))
        {
            let length = new.js_number();
            if !(0.0..=f64::from(u32::MAX)).contains(&length) || length.fract() != 0.0 {
                return Err(self.error(at, "Tamanho de vetor inválido."));
            }
            let Object::List(items, _) = &mut *object.borrow_mut() else {
                unreachable!();
            };
            return self.resize_list(items, length as usize, Value::Undefined, at);
        }
        let numeric = index.js_number();
        match &mut *object.borrow_mut() {
            Object::List(items, properties) => {
                let n = wrap_index(numeric, items.len());
                let array_index = (0.0..f64::from(u32::MAX)).contains(&n)
                    && n.fract() == 0.0
                    && (numeric < 0.0 || number_key(&key, n));
                let filled = n.ceil();
                if filled > f64::from(u32::MAX) {
                    return Err(self.error(at, "Índice excede o tamanho máximo do vetor."));
                }
                let length = (filled as usize + usize::from(array_index)).max(items.len());
                self.resize_list(items, length, Value::Null, at)?;
                if array_index {
                    items[n as usize] = new;
                } else {
                    let key = if numeric < 0.0 && !items.is_empty() {
                        key_string(&Value::Number(n))
                    } else {
                        key
                    };
                    properties.insert(key, new);
                }
            }
            Object::Dictionary(fields)
            | Object::Module(fields)
            | Object::Function(_, fields)
            | Object::Class(_, fields)
            | Object::Instance(_, _, fields) => {
                fields.insert(key, new);
            }
        }
        Ok(())
    }

    fn resize_list(
        &mut self,
        items: &mut Vec<Value>,
        length: usize,
        fill: Value,
        at: Location,
    ) -> Result<()> {
        if length <= items.len() {
            items.truncate(length);
            return Ok(());
        }
        let original = items.len();
        items
            .try_reserve(length - original)
            .map_err(|_| self.error(at, "Não foi possível expandir o vetor."))?;
        while items.len() < length {
            if let Err(signal) = self.poll(at) {
                items.truncate(original);
                return Err(signal);
            }
            items.resize(items.len().saturating_add(4096).min(length), fill.clone());
        }
        Ok(())
    }

    fn call(&mut self, callee: Value, mut args: Vec<Value>, at: Location) -> Result<Value> {
        if let Value::Native(native) = callee {
            if args.len() < native.arity {
                args.resize(native.arity, Value::Null);
            }
            return match builtins::invoke(self, &native, &args, at) {
                Err(Signal::Break(_) | Signal::Continue(_)) => {
                    Err(self.error(at, "Controle de laço fora da função integrada."))
                }
                result => result,
            };
        }
        let Value::Object(object) = callee else {
            return Err(self.error(at, "Só pode chamar função ou classe."));
        };
        let routine = match &*object.borrow() {
            Object::Function(routine, _) => Some(routine.clone()),
            Object::Class(..) => None,
            _ => return Err(self.error(at, "Só pode chamar função ou classe.")),
        };
        if let Some(routine) = routine {
            let params = &routine.declaration.parameters;
            if args.len() < params.len() {
                args.resize(params.len(), Value::Null);
            } else if params.last().is_some_and(|parameter| parameter.wildcard) {
                let tail = args.split_off(params.len() - 1);
                args.push(self.list(tail));
            }
            return self.call_routine(&routine, &args, at);
        }
        let instance = self.object(Object::Instance(
            object.clone(),
            Fields::new(),
            Fields::new(),
        ));
        if let Some(initializer) = find_method(&object, "construtor") {
            let arity = initializer.declaration.parameters.len();
            if args.len() < arity {
                args.resize(arity, Value::Null);
            }
            let bound = self.bind(initializer, instance.clone());
            self.call_callback(&bound, &args, at)?;
        }
        Ok(instance)
    }

    pub fn call_callback(
        &mut self,
        callback: &Value,
        args: &[Value],
        at: Location,
    ) -> Result<Value> {
        if let Value::Object(object) = callback {
            let routine = match &*object.borrow() {
                Object::Function(routine, _) => Some(routine.clone()),
                _ => None,
            };
            if let Some(routine) = routine {
                return self.call_routine(&routine, args, at);
            }
        }
        Err(self.error(at, "O segundo parâmetro deve ser uma função."))
    }

    fn call_routine(&mut self, routine: &Routine, args: &[Value], at: Location) -> Result<Value> {
        if self.call_stack.len() >= 128 {
            return Err(self.error(at, "Limite de chamadas de função atingido."));
        }
        let environment = self.nested(routine.closure.clone());
        for (index, parameter) in routine.declaration.parameters.iter().enumerate() {
            let value = match args.get(index) {
                Some(Value::Null) => match &parameter.default {
                    Some(Expr::Literal(value)) => value.clone(),
                    Some(_) => Value::Undefined,
                    None => Value::Null,
                },
                Some(value) => value.clone(),
                None => Value::Undefined,
            };
            environment
                .borrow_mut()
                .values
                .insert(parameter.name.text.clone(), value);
        }
        let original = std::mem::replace(&mut self.path, routine.path.clone());
        self.call_stack.push(routine.name.clone());
        let returned = self.in_scope(&routine.declaration.body, environment);
        self.call_stack.pop();
        self.path = original;
        let value = match returned {
            Err(Signal::Return(value, _)) => value,
            Ok(()) => Value::Null,
            Err(other) => return Err(other),
        };
        if routine.initializer {
            Ok(find(&routine.closure, "isto").unwrap_or(Value::Undefined))
        } else {
            Ok(value)
        }
    }

    fn import(&mut self, path: Value, at: Location) -> Result<Value> {
        let Value::Text(path) = path else {
            return Err(self.error(at, "O caminho da importação deve ser um texto."));
        };
        let path = String::from_utf16_lossy(&path);
        if let Some(library) = builtins::module(&path) {
            return Ok(self.object(library));
        }
        if self.imports >= 64 {
            return Err(self.error(at, "Limite de importações aninhadas atingido."));
        }
        let module = self
            .host
            .load_module(self.module_path.as_deref(), &path)
            .map_err(|error| self.host_error(at, error))?;
        let program = crate::parse(&module.source)
            .and_then(ParsedProgram::resolve)
            .map_err(|mut error| {
                error.path = Some(module.path.clone());
                Signal::Error(error)
            })?;
        let root = self.state.global_scope();
        let caller_globals = std::mem::replace(&mut self.globals, root.clone());
        let caller_module = self.module_path.replace(module.path.clone());
        let original = self.path.replace(module.path);
        self.imports += 1;
        let result = self.in_scope(&program.0, root.clone());
        self.imports -= 1;
        self.path = original;
        self.module_path = caller_module;
        self.globals = caller_globals;
        result?;
        let exported = find(&root, "exports").unwrap_or(Value::Undefined);
        if let Value::Object(object) = &exported
            && let Object::Dictionary(fields) = &*object.borrow()
        {
            return Ok(self.object(Object::Module(fields.clone())));
        }
        Ok(exported)
    }

    pub fn random(&mut self) -> f64 {
        let state = self
            .state
            .random
            .get_or_insert_with(|| self.host.random_seed());
        *state = state.wrapping_add(0x9e3779b97f4a7c15);
        let bits = (*state ^ (*state >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        let bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((bits ^ (bits >> 31)) >> 11) as f64 / 9_007_199_254_740_992.0
    }
}

fn ancestor(current: &Environment, depth: usize) -> Option<Environment> {
    let mut scope = current.clone();
    for _ in 0..depth {
        let parent = scope.borrow().parent.clone()?;
        scope = parent;
    }
    Some(scope)
}

fn global(current: &Environment) -> Environment {
    let mut scope = current.clone();
    loop {
        let Some(parent) = scope.borrow().parent.clone() else {
            return scope.clone();
        };
        scope = parent;
    }
}

fn find_scope(current: &Environment, name: &str) -> Option<Environment> {
    let mut scope = current.clone();
    loop {
        let parent = {
            let current = scope.borrow();
            if current
                .values
                .get(name)
                .is_some_and(|value| !matches!(value, Value::Undefined))
            {
                return Some(scope.clone());
            }
            current.parent.clone()?
        };
        scope = parent;
    }
}

fn find(scope: &Environment, name: &str) -> Option<Value> {
    find_scope(scope, name)?.borrow().values.get(name).cloned()
}

fn find_method(class: &ObjectRef, name: &str) -> Option<Routine> {
    let mut class = class.clone();
    loop {
        let parent = {
            let object = class.borrow();
            let Object::Class(data, _) = &*object else {
                return None;
            };
            if let Some(method) = data.methods.get(name) {
                return Some(method.clone());
            }
            data.parent.clone()?
        };
        class = parent;
    }
}

fn key_string(value: &Value) -> Rc<[u16]> {
    match value {
        Value::Text(text) => text.clone(),
        _ => value.js_string().encode_utf16().collect(),
    }
}

fn number_key(key: &[u16], number: f64) -> bool {
    crate::value::number_string(number)
        .encode_utf16()
        .eq(key.iter().copied())
}

fn wrap_index(index: f64, len: usize) -> f64 {
    if index < 0.0 && len != 0 {
        index.rem_euclid(len as f64)
    } else {
        index
    }
}

pub(super) fn contains_text(haystack: &[u16], needle: &[u16]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|piece| piece == needle)
}

pub(super) fn pow(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() || (base.abs() == 1.0 && exponent.is_infinite()) {
        f64::NAN
    } else {
        base.powf(exponent)
    }
}
