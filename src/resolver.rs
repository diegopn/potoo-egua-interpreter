use std::collections::HashMap;
use std::rc::Rc;

use crate::lexer::{Kind, Token};
use crate::parser::{Expr, Function, Statement, Stmt};
use crate::{Diagnostic, Phase};

#[derive(Clone, Copy, PartialEq)]
enum ClassContext {
    None,
    Class,
    Subclass,
}

struct Resolver<'src> {
    scopes: Vec<HashMap<&'src str, bool>>,
    in_function: bool,
    class: ClassContext,
}

pub(super) fn resolve(statements: &mut [Stmt]) -> Result<(), Diagnostic> {
    Resolver {
        scopes: Vec::new(),
        in_function: false,
        class: ClassContext::None,
    }
    .statements(statements)
}

impl<'src> Resolver<'src> {
    fn statements(&mut self, statements: &'src mut [Stmt]) -> Result<(), Diagnostic> {
        for statement in statements {
            self.statement(statement)?;
        }
        Ok(())
    }

    fn statement(&mut self, statement: &'src mut Stmt) -> Result<(), Diagnostic> {
        match &mut statement.kind {
            Statement::Var(name, initializer) => {
                self.declare(name)?;
                self.optional(initializer.as_mut())?;
                self.define(&name.text);
            }
            Statement::Evaluate(expr) | Statement::Print(expr) => self.expression(expr)?,
            Statement::Block(statements) => {
                self.scopes.push(HashMap::new());
                self.statements(statements)?;
                self.scopes.pop();
            }
            Statement::If {
                branches,
                otherwise,
            } => {
                for (condition, branch) in branches {
                    self.expression(condition)?;
                    self.statement(branch)?;
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
                ..
            } => {
                if let Some(initializer) = initializer {
                    self.statement(initializer)?;
                }
                self.optional(condition.as_mut())?;
                self.optional(step.as_mut())?;
                self.statement(body)?;
            }
            Statement::Switch {
                cases, otherwise, ..
            } => {
                // The tagged resolver visits branch bodies only.
                for (_, statements) in cases {
                    self.statements(statements)?;
                }
                if let Some(otherwise) = otherwise {
                    self.statements(otherwise)?;
                }
            }
            Statement::Try {
                body,
                catch,
                otherwise,
                finally,
            } => {
                // The tagged resolver visits these lists in the surrounding scope.
                self.statements(body)?;
                for block in [catch, otherwise, finally].into_iter().flatten() {
                    self.statements(block)?;
                }
            }
            Statement::Function(name, declaration) => {
                self.declare(name)?;
                self.define(&name.text);
                self.function(declaration)?;
            }
            Statement::Class(name, parent, methods) => {
                self.declare(name)?;
                self.define(&name.text);
                let enclosing = self.class;
                self.class = ClassContext::Class;
                let inherits = parent.is_some();
                if let Some(parent) = parent {
                    if matches!(parent, Expr::Name(token, _) if token.text == name.text) {
                        return Err(name
                            .location
                            .error(Phase::Resolution, "Uma classe não pode herdar de si mesma."));
                    }
                    self.class = ClassContext::Subclass;
                    self.expression(parent)?;
                    self.scopes.push(HashMap::from([("super", true)]));
                }
                self.scopes.push(HashMap::from([("isto", true)]));
                for (_, method) in methods {
                    self.function(method)?;
                }
                self.scopes.pop();
                if inherits {
                    self.scopes.pop();
                }
                self.class = enclosing;
            }
            Statement::Return(value) => {
                if !self.in_function {
                    return Err(statement.location.error(
                        Phase::Resolution,
                        "Não é possível retornar do código do escopo superior.",
                    ));
                }
                self.optional(value.as_mut())?;
            }
            Statement::Break | Statement::Continue => {}
        }
        Ok(())
    }

    fn expression(&mut self, expr: &'src mut Expr) -> Result<(), Diagnostic> {
        match expr {
            Expr::Literal(_) => {}
            // The tagged resolver leaves indexed assignments unresolved.
            Expr::SetIndex(..) => {}
            Expr::Name(token, depth) => {
                if token.kind == Kind::This && self.class == ClassContext::None {
                    return Err(token
                        .location
                        .error(Phase::Resolution, "Não pode usar 'isto' fora da classe."));
                }
                if self
                    .scopes
                    .last()
                    .and_then(|scope| scope.get(token.text.as_str()))
                    == Some(&false)
                {
                    return Err(token.location.error(
                        Phase::Resolution,
                        "Não é possível ler a variável local em seu próprio inicializador.",
                    ));
                }
                *depth = self.depth(&token.text);
            }
            Expr::Super(token, _, depth) => {
                if self.class != ClassContext::Subclass {
                    return Err(token.location.error(
                        Phase::Resolution,
                        "'super' requer uma classe com superclasse.",
                    ));
                }
                *depth = self.depth(&token.text);
            }
            Expr::Assign(token, depth, value) => {
                self.expression(value)?;
                *depth = self.depth(&token.text);
            }
            Expr::Group(inner)
            | Expr::Unary(_, inner)
            | Expr::Get(inner, _)
            | Expr::Import(inner, _) => {
                self.expression(inner)?;
            }
            Expr::Binary(left, _, right) | Expr::Index(left, _, right) => {
                self.expression(left)?;
                self.expression(right)?;
            }
            Expr::Dictionary(entries) => {
                for (key, value) in entries {
                    self.expression(key)?;
                    self.expression(value)?;
                }
            }
            Expr::Function(function) => self.function(function)?,
            Expr::Call(callee, _, arguments) => {
                self.expression(callee)?;
                for argument in arguments {
                    self.expression(argument)?;
                }
            }
            Expr::List(items) => {
                for item in items {
                    self.expression(item)?;
                }
            }
            Expr::Set(object, _, value) => {
                self.expression(value)?;
                self.expression(object)?;
            }
        }
        Ok(())
    }

    fn optional(&mut self, expr: Option<&'src mut Expr>) -> Result<(), Diagnostic> {
        if let Some(expr) = expr {
            self.expression(expr)?;
        }
        Ok(())
    }

    fn function(&mut self, function: &'src mut Rc<Function>) -> Result<(), Diagnostic> {
        let function = Rc::get_mut(function).expect("the resolver owns each declaration");
        let enclosing = self.in_function;
        self.in_function = true;
        self.scopes.push(HashMap::new());
        for parameter in &function.parameters {
            self.declare(&parameter.name)?;
            self.define(&parameter.name.text);
        }
        self.statements(&mut function.body)?;
        self.scopes.pop();
        self.in_function = enclosing;
        Ok(())
    }

    fn declare(&mut self, name: &'src Token) -> Result<(), Diagnostic> {
        if let Some(scope) = self.scopes.last_mut()
            && scope.insert(&name.text, false).is_some()
        {
            return Err(name.location.error(
                Phase::Resolution,
                "Variável com esse nome já declarada neste escopo.",
            ));
        }
        Ok(())
    }

    fn define(&mut self, name: &'src str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, true);
        }
    }

    fn depth(&self, name: &str) -> Option<usize> {
        // The tagged resolver overwrites earlier matches, keeping the outermost local.
        self.scopes
            .iter()
            .position(|scope| scope.contains_key(name))
            .map(|index| self.scopes.len() - 1 - index)
    }
}
