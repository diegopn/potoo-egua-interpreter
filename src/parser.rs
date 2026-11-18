use std::rc::Rc;

use crate::lexer::{Kind, Token};
use crate::value::Value;
use crate::{Diagnostic, Location, Phase};

pub(super) struct Stmt {
    pub location: Location,
    pub kind: Statement,
}

pub(super) enum Statement {
    Var(Token, Option<Expr>),
    Print(Expr),
    Evaluate(Expr),
    Block(Vec<Stmt>),
    If {
        branches: Vec<(Expr, Stmt)>,
        otherwise: Option<Box<Stmt>>,
    },
    Loop {
        initializer: Option<Box<Stmt>>,
        condition: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
        post_condition: bool,
    },
    Switch {
        subject: Expr,
        cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
        otherwise: Option<Vec<Stmt>>,
    },
    Try {
        body: Vec<Stmt>,
        catch: Option<Vec<Stmt>>,
        otherwise: Option<Vec<Stmt>>,
        finally: Option<Vec<Stmt>>,
    },
    Function(Token, Rc<Function>),
    Class(Token, Option<Expr>, Vec<(Token, Rc<Function>)>),
    Return(Option<Expr>),
    Break,
    Continue,
}

pub(super) struct Function {
    pub parameters: Vec<Parameter>,
    pub body: Vec<Stmt>,
}

pub(super) struct Parameter {
    pub name: Token,
    pub default: Option<Expr>,
    pub wildcard: bool,
}

pub(super) enum Expr {
    Literal(Value),
    Name(Token, Option<usize>),
    Super(Token, Token, Option<usize>),
    Assign(Token, Option<usize>, Box<Expr>),
    Group(Box<Expr>),
    Unary(Token, Box<Expr>),
    Binary(Box<Expr>, Token, Box<Expr>),
    List(Vec<Expr>),
    Dictionary(Vec<(Expr, Expr)>),
    Function(Rc<Function>),
    Call(Box<Expr>, Location, Vec<Expr>),
    Get(Box<Expr>, Token),
    Set(Box<Expr>, Token, Box<Expr>),
    Index(Box<Expr>, Location, Box<Expr>),
    SetIndex(Box<Expr>, Location, Box<Expr>, Box<Expr>),
    Import(Box<Expr>, Location),
}

struct Parser<'tokens> {
    tokens: &'tokens [Token],
    index: usize,
    loop_depth: usize,
}

pub(super) fn parse(tokens: &[Token]) -> Result<Vec<Stmt>, Diagnostic> {
    let mut parser = Parser {
        tokens,
        index: 0,
        loop_depth: 0,
    };
    let mut program = Vec::new();
    while !parser.at(Kind::End) {
        program.push(parser.declaration()?);
    }
    Ok(program)
}

impl Parser<'_> {
    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }

    fn at(&self, kind: Kind) -> bool {
        self.current().kind == kind
    }

    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        if token.kind != Kind::End {
            self.index += 1;
        }
        token
    }

    fn eat(&mut self, kind: Kind) -> Option<Token> {
        self.at(kind).then(|| self.advance())
    }

    fn expect(&mut self, kind: Kind, message: &str) -> Result<Token, Diagnostic> {
        self.eat(kind).ok_or_else(|| self.syntax_error(message))
    }

    fn syntax_error(&self, message: &str) -> Diagnostic {
        self.current().location.error(Phase::Syntax, message)
    }

    fn declaration(&mut self) -> Result<Stmt, Diagnostic> {
        let location = self.current().location;
        let kind = match self.current().kind {
            Kind::Var => {
                self.advance();
                self.variable()?
            }
            Kind::Function => {
                self.advance();
                let name = self.expect(Kind::Name, "Esperado nome da função.")?;
                Statement::Function(name, self.function()?)
            }
            Kind::Class => {
                self.advance();
                self.class()?
            }
            _ => return self.statement(),
        };
        Ok(Stmt { location, kind })
    }

    fn variable(&mut self) -> Result<Statement, Diagnostic> {
        let name = self.expect(Kind::Name, "Esperado nome de variável.")?;
        let initializer = if self.eat(Kind::Assign).is_some() {
            Some(self.expression()?)
        } else {
            None
        };
        self.expect(Kind::Semicolon, "Esperado ';' após a declaração.")?;
        Ok(Statement::Var(name, initializer))
    }

    fn class(&mut self) -> Result<Statement, Diagnostic> {
        let name = self.expect(Kind::Name, "Esperado nome da classe.")?;
        let parent = if self.eat(Kind::Inherits).is_some() {
            let parent = self.expect(Kind::Name, "Esperado nome da superclasse.")?;
            Some(Expr::Name(parent, None))
        } else {
            None
        };
        self.expect(Kind::LeftBrace, "Esperado '{' antes dos métodos.")?;
        let mut methods = Vec::new();
        while !self.at(Kind::RightBrace) {
            let name = self.expect(Kind::Name, "Esperado nome do método.")?;
            methods.push((name, self.function()?));
        }
        self.advance();
        Ok(Statement::Class(name, parent, methods))
    }

    fn function(&mut self) -> Result<Rc<Function>, Diagnostic> {
        self.expect(Kind::LeftParen, "Esperado '(' antes dos parâmetros.")?;
        let mut parameters = Vec::new();
        if !self.at(Kind::RightParen) {
            loop {
                if parameters.len() == 255 {
                    return Err(self.syntax_error("Não pode haver mais de 255 parâmetros."));
                }
                let wildcard = self.eat(Kind::Star).is_some();
                let name = self.expect(Kind::Name, "Esperado nome do parâmetro.")?;
                let default = if self.eat(Kind::Assign).is_some() {
                    Some(self.primary()?)
                } else {
                    None
                };
                parameters.push(Parameter {
                    name,
                    default,
                    wildcard,
                });
                if wildcard || self.eat(Kind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(Kind::RightParen, "Esperado ')' após os parâmetros.")?;
        Ok(Rc::new(Function {
            parameters,
            body: self.braced_block()?,
        }))
    }

    fn braced_block(&mut self) -> Result<Vec<Stmt>, Diagnostic> {
        self.expect(Kind::LeftBrace, "Esperado '{' no início do bloco.")?;
        self.block()
    }

    fn block(&mut self) -> Result<Vec<Stmt>, Diagnostic> {
        let mut statements = Vec::new();
        while !matches!(self.current().kind, Kind::RightBrace | Kind::End) {
            statements.push(self.declaration()?);
        }
        self.expect(Kind::RightBrace, "Esperado '}' no fim do bloco.")?;
        Ok(statements)
    }

    fn statement(&mut self) -> Result<Stmt, Diagnostic> {
        let mut token = self.current().clone();
        // Accept the spelling shown in the documentation without reserving a new name.
        if token.kind == Kind::Name
            && token.text == "faça"
            && self.tokens[self.index + 1].kind == Kind::LeftBrace
        {
            token.kind = Kind::Do;
        }
        let kind = match token.kind {
            Kind::LeftBrace => {
                self.advance();
                Statement::Block(self.block()?)
            }
            Kind::Print => {
                self.advance();
                let value = self.parenthesized()?;
                self.expect(Kind::Semicolon, "Esperado ';' após 'escreva'.")?;
                Statement::Print(value)
            }
            Kind::If => {
                self.advance();
                let mut branches = vec![(self.parenthesized()?, self.statement()?)];
                while self.eat(Kind::ElseIf).is_some() {
                    branches.push((self.parenthesized()?, self.statement()?));
                }
                let otherwise = if self.eat(Kind::Else).is_some() {
                    Some(Box::new(self.statement()?))
                } else {
                    None
                };
                Statement::If {
                    branches,
                    otherwise,
                }
            }
            Kind::For | Kind::While | Kind::Do | Kind::Switch => {
                self.advance();
                self.loop_depth += 1;
                let result = match token.kind {
                    Kind::For => self.for_loop(),
                    Kind::While => self.while_loop(),
                    Kind::Do => self.do_loop(),
                    Kind::Switch => self.switch(),
                    _ => unreachable!(),
                };
                self.loop_depth -= 1;
                result?
            }
            Kind::Try => {
                self.advance();
                let body = self.braced_block()?;
                let catch = self.optional_block(Kind::Catch)?;
                let otherwise = self.optional_block(Kind::Else)?;
                let finally = self.optional_block(Kind::Finally)?;
                Statement::Try {
                    body,
                    catch,
                    otherwise,
                    finally,
                }
            }
            Kind::Return => {
                self.advance();
                let value = if self.at(Kind::Semicolon) {
                    None
                } else {
                    Some(self.expression()?)
                };
                self.expect(Kind::Semicolon, "Esperado ';' após o retorno.")?;
                Statement::Return(value)
            }
            Kind::Break | Kind::Continue => {
                self.advance();
                if self.loop_depth == 0 {
                    return Err(token.location.error(
                        Phase::Syntax,
                        format!(
                            "'{}' só pode ser usado dentro de um laço ou 'escolha'.",
                            token.text
                        ),
                    ));
                }
                self.expect(Kind::Semicolon, "Esperado ';' após o comando.")?;
                if token.kind == Kind::Break {
                    Statement::Break
                } else {
                    Statement::Continue
                }
            }
            _ => self.expression_statement()?,
        };
        Ok(Stmt {
            location: token.location,
            kind,
        })
    }

    fn expression_statement(&mut self) -> Result<Statement, Diagnostic> {
        let value = self.expression()?;
        self.expect(Kind::Semicolon, "Esperado ';' após a expressão.")?;
        Ok(Statement::Evaluate(value))
    }

    fn for_loop(&mut self) -> Result<Statement, Diagnostic> {
        self.expect(Kind::LeftParen, "Esperado '(' após 'para'.")?;
        let location = self.current().location;
        let initializer = if self.eat(Kind::Semicolon).is_some() {
            None
        } else {
            let kind = if self.eat(Kind::Var).is_some() {
                self.variable()?
            } else {
                self.expression_statement()?
            };
            Some(Box::new(Stmt { location, kind }))
        };
        let condition = self.optional_expression(Kind::Semicolon)?;
        let step = self.optional_expression(Kind::RightParen)?;
        Ok(Statement::Loop {
            initializer,
            condition,
            step,
            body: Box::new(self.statement()?),
            post_condition: false,
        })
    }

    fn while_loop(&mut self) -> Result<Statement, Diagnostic> {
        Ok(Statement::Loop {
            initializer: None,
            condition: Some(self.parenthesized()?),
            step: None,
            body: Box::new(self.statement()?),
            post_condition: false,
        })
    }

    fn do_loop(&mut self) -> Result<Statement, Diagnostic> {
        let body = Box::new(self.statement()?);
        self.expect(Kind::While, "Esperado 'enquanto' após o corpo de 'fazer'.")?;
        let condition = self.parenthesized()?;
        self.eat(Kind::Semicolon);
        Ok(Statement::Loop {
            initializer: None,
            condition: Some(condition),
            step: None,
            body,
            post_condition: true,
        })
    }

    fn optional_expression(&mut self, end: Kind) -> Result<Option<Expr>, Diagnostic> {
        let result = if self.at(end) {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect(end, "Cláusula de 'para' sem delimitador de fechamento.")?;
        Ok(result)
    }

    fn optional_block(&mut self, kind: Kind) -> Result<Option<Vec<Stmt>>, Diagnostic> {
        if self.eat(kind).is_some() {
            Ok(Some(self.braced_block()?))
        } else {
            Ok(None)
        }
    }

    fn switch(&mut self) -> Result<Statement, Diagnostic> {
        let subject = self.parenthesized()?;
        self.expect(Kind::LeftBrace, "Esperado '{' antes dos casos.")?;
        let mut cases = Vec::new();
        let mut otherwise = None;
        while !self.at(Kind::RightBrace) {
            if self.eat(Kind::Case).is_some() {
                let mut labels = Vec::new();
                loop {
                    labels.push(self.expression()?);
                    self.expect(Kind::Colon, "Esperado ':' após o 'caso'.")?;
                    if self.eat(Kind::Case).is_none() {
                        break;
                    }
                }
                cases.push((labels, self.case_body()?));
            } else if self.eat(Kind::Default).is_some() && otherwise.is_none() {
                self.expect(Kind::Colon, "Esperado ':' após 'padrão'.")?;
                otherwise = Some(self.case_body()?);
            } else {
                return Err(self.syntax_error("Esperado 'caso', um único 'padrão' ou '}'."));
            }
        }
        self.advance();
        Ok(Statement::Switch {
            subject,
            cases,
            otherwise,
        })
    }

    fn case_body(&mut self) -> Result<Vec<Stmt>, Diagnostic> {
        let mut body = vec![self.statement()?];
        while !matches!(
            self.current().kind,
            Kind::Case | Kind::Default | Kind::RightBrace | Kind::End
        ) {
            body.push(self.statement()?);
        }
        Ok(body)
    }

    fn parenthesized(&mut self) -> Result<Expr, Diagnostic> {
        self.expect(Kind::LeftParen, "Esperado '('.")?;
        let expression = self.expression()?;
        self.expect(Kind::RightParen, "Esperado ')' após a expressão.")?;
        Ok(expression)
    }

    fn expression(&mut self) -> Result<Expr, Diagnostic> {
        let expression = self.binary(0)?;
        let Some(operator) = self.eat(Kind::Assign) else {
            return Ok(expression);
        };
        let value = Box::new(self.expression()?);
        match expression {
            Expr::Name(name, _) if name.kind == Kind::Name => Ok(Expr::Assign(name, None, value)),
            Expr::Get(object, name) => Ok(Expr::Set(object, name, value)),
            Expr::Index(object, location, index) => {
                Ok(Expr::SetIndex(object, location, index, value))
            }
            _ => Err(operator
                .location
                .error(Phase::Syntax, "Alvo de atribuição inválido.")),
        }
    }

    fn binary(&mut self, minimum: u8) -> Result<Expr, Diagnostic> {
        let mut left = self.unary()?;
        while let Some(precedence) = precedence(self.current().kind) {
            if precedence < minimum {
                break;
            }
            let operator = self.advance();
            let right = self.binary(precedence + 1)?;
            left = Expr::Binary(Box::new(left), operator, Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, Diagnostic> {
        if matches!(self.current().kind, Kind::Minus | Kind::Bang | Kind::BitNot) {
            let operator = self.advance();
            return Ok(Expr::Unary(operator, Box::new(self.unary()?)));
        }
        let mut expression = self.primary()?;
        loop {
            expression = match self.current().kind {
                Kind::LeftParen => {
                    self.advance();
                    let mut arguments = Vec::new();
                    if !self.at(Kind::RightParen) {
                        loop {
                            if arguments.len() == 255 {
                                return Err(
                                    self.syntax_error("Não pode haver mais de 255 argumentos.")
                                );
                            }
                            arguments.push(self.expression()?);
                            if self.eat(Kind::Comma).is_none() {
                                break;
                            }
                        }
                    }
                    let close =
                        self.expect(Kind::RightParen, "Esperado ')' após os argumentos.")?;
                    Expr::Call(Box::new(expression), close.location, arguments)
                }
                Kind::Dot => {
                    self.advance();
                    // The time library documents a method named `pausa`.
                    let name = if self.at(Kind::Break) {
                        self.advance()
                    } else {
                        self.expect(Kind::Name, "Esperado nome após '.'.")?
                    };
                    Expr::Get(Box::new(expression), name)
                }
                Kind::LeftSquare => {
                    self.advance();
                    let index = self.expression()?;
                    let close = self.expect(Kind::RightSquare, "Esperado ']' após o índice.")?;
                    Expr::Index(Box::new(expression), close.location, Box::new(index))
                }
                _ => break,
            }
        }
        Ok(expression)
    }

    fn primary(&mut self) -> Result<Expr, Diagnostic> {
        let token = self.advance();
        Ok(match token.kind {
            Kind::Name | Kind::This => Expr::Name(token, None),
            Kind::Super => {
                self.expect(Kind::Dot, "Esperado '.' após 'super'.")?;
                let method = self.expect(Kind::Name, "Esperado nome do método da superclasse.")?;
                Expr::Super(token, method, None)
            }
            Kind::Number => {
                Expr::Literal(Value::Number(token.text.parse().map_err(|_| {
                    token.location.error(Phase::Syntax, "Número inválido.")
                })?))
            }
            Kind::Text => Expr::Literal(Value::text(&token.text[1..token.text.len() - 1])),
            Kind::True => Expr::Literal(Value::Bool(true)),
            Kind::False => Expr::Literal(Value::Bool(false)),
            Kind::Null => Expr::Literal(Value::Null),
            Kind::LeftParen => {
                let inner = self.expression()?;
                self.expect(Kind::RightParen, "Esperado ')' após a expressão.")?;
                Expr::Group(Box::new(inner))
            }
            Kind::Function => Expr::Function(self.function()?),
            Kind::Import => Expr::Import(Box::new(self.parenthesized()?), token.location),
            Kind::LeftSquare => {
                let mut items = Vec::new();
                while !self.at(Kind::RightSquare) {
                    items.push(self.expression()?);
                    if !self.at(Kind::RightSquare) {
                        self.expect(Kind::Comma, "Esperado ',' entre os itens.")?;
                    }
                }
                self.advance();
                Expr::List(items)
            }
            Kind::LeftBrace => {
                let mut entries = Vec::new();
                while !self.at(Kind::RightBrace) {
                    let key = self.expression()?;
                    self.expect(Kind::Colon, "Esperado ':' entre a chave e o valor.")?;
                    entries.push((key, self.expression()?));
                    if !self.at(Kind::RightBrace) {
                        self.expect(Kind::Comma, "Esperado ',' entre as entradas.")?;
                    }
                }
                self.advance();
                Expr::Dictionary(entries)
            }
            _ => return Err(token.location.error(Phase::Syntax, "Esperado expressão.")),
        })
    }
}

fn precedence(kind: Kind) -> Option<u8> {
    use Kind::*;
    Some(match kind {
        Or => 0,
        And => 1,
        In => 2,
        Equal | NotEqual => 3,
        Less | LessEqual | Greater | GreaterEqual => 4,
        BitOr | BitXor => 5,
        BitAnd => 6,
        ShiftLeft | ShiftRight => 7,
        Plus | Minus => 8,
        Star | Slash | Percent => 9,
        Power => 10,
        _ => return None,
    })
}
