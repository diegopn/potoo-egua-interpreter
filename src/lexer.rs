use crate::{Diagnostic, Location, Phase};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Semicolon,
    LeftSquare,
    RightSquare,
    Comma,
    Dot,
    Colon,
    BitNot,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
    Plus,
    Minus,
    Star,
    Power,
    Slash,
    Percent,
    Bang,
    NotEqual,
    Assign,
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    In,
    And,
    Or,
    Var,
    Print,
    If,
    ElseIf,
    Else,
    While,
    For,
    Break,
    Continue,
    True,
    False,
    Null,
    Name,
    Number,
    Text,
    Class,
    Function,
    Return,
    Super,
    This,
    Do,
    Switch,
    Case,
    Default,
    Import,
    Try,
    Catch,
    Finally,
    Inherits,
    End,
}

#[derive(Clone, Debug)]
pub(super) struct Token {
    pub kind: Kind,
    pub text: String,
    pub location: Location,
}

struct Scanner<'src> {
    remaining: &'src str,
    location: Location,
}

pub(super) fn scan(source: &str) -> Result<Vec<Token>, Diagnostic> {
    let mut scanner = Scanner {
        remaining: source,
        location: Location { line: 1, column: 1 },
    };
    let mut tokens = Vec::new();
    while !scanner.remaining.is_empty() {
        let start = scanner.remaining;
        let location = scanner.location;
        if let Some(kind) = scanner.token()? {
            let end = start.len() - scanner.remaining.len();
            tokens.push(Token {
                kind,
                text: start[..end].to_owned(),
                location,
            });
        }
    }
    tokens.push(Token {
        kind: Kind::End,
        text: String::new(),
        location: scanner.location,
    });
    Ok(tokens)
}

impl Scanner<'_> {
    fn take(&mut self) -> Option<char> {
        let ch = self.remaining.chars().next()?;
        self.remaining = &self.remaining[ch.len_utf8()..];
        if ch == '\n' {
            self.location.line += 1;
            self.location.column = 1;
        } else {
            self.location.column += 1;
        }
        Some(ch)
    }

    fn eat(&mut self, ch: char) -> bool {
        if self.remaining.starts_with(ch) {
            self.take();
            return true;
        }
        false
    }

    fn take_while(&mut self, predicate: impl Fn(char) -> bool) {
        while self.remaining.chars().next().is_some_and(&predicate) {
            self.take();
        }
    }

    fn token(&mut self) -> Result<Option<Kind>, Diagnostic> {
        let start = self.remaining;
        let location = self.location;
        let Some(ch) = self.take() else {
            return Ok(None);
        };
        use Kind::*;
        let kind = match ch {
            ' ' | '\r' | '\t' | '\n' => return Ok(None),
            '/' if self.eat('/') => {
                self.take_while(|ch| ch != '\n');
                return Ok(None);
            }
            '(' => LeftParen,
            ')' => RightParen,
            '{' => LeftBrace,
            '}' => RightBrace,
            ';' => Semicolon,
            '+' => Plus,
            '-' => Minus,
            '*' if self.eat('*') => Power,
            '*' => Star,
            '/' => Slash,
            '%' => Percent,
            '!' if self.eat('=') => NotEqual,
            '!' => Bang,
            '=' if self.eat('=') => Equal,
            '=' => Assign,
            '<' if self.eat('=') => LessEqual,
            '>' if self.eat('=') => GreaterEqual,
            '<' if self.eat('<') => ShiftLeft,
            '>' if self.eat('>') => ShiftRight,
            '<' => Less,
            '>' => Greater,
            '[' => LeftSquare,
            ']' => RightSquare,
            ',' => Comma,
            '.' => Dot,
            ':' => Colon,
            '~' => BitNot,
            '&' => BitAnd,
            '|' => BitOr,
            '^' => BitXor,
            quote @ ('\'' | '"') => {
                self.take_while(|ch| ch != quote);
                if !self.eat(quote) {
                    return Err(location.error(Phase::Lexical, "Texto sem aspas de fechamento."));
                }
                Text
            }
            '0'..='9' => {
                self.take_while(|ch| ch.is_ascii_digit());
                if let Some(fraction) = self.remaining.strip_prefix('.')
                    && fraction.starts_with(|ch: char| ch.is_ascii_digit())
                {
                    self.take();
                    self.take_while(|ch| ch.is_ascii_digit());
                }
                Number
            }
            ch if identifier_letter(ch) => {
                self.take_while(|ch| identifier_letter(ch) || ch.is_ascii_digit());
                let end = start.len() - self.remaining.len();
                match &start[..end] {
                    "var" => Var,
                    "escreva" => Print,
                    "se" => If,
                    "senãose" => ElseIf,
                    "senão" => Else,
                    "enquanto" => While,
                    "para" => For,
                    "pausa" => Break,
                    "continua" => Continue,
                    "e" => And,
                    "ou" => Or,
                    "verdadeiro" => True,
                    "falso" => False,
                    "nulo" => Null,
                    "em" => In,
                    "classe" => Class,
                    "função" => Function,
                    "retorna" => Return,
                    "super" => Super,
                    "isto" => This,
                    "fazer" => Do,
                    "escolha" => Switch,
                    "caso" => Case,
                    "padrão" => Default,
                    "importar" => Import,
                    "tente" => Try,
                    "pegue" => Catch,
                    "finalmente" => Finally,
                    "herda" => Inherits,
                    _ => Name,
                }
            }
            _ => {
                return Err(location.error(Phase::Lexical, format!("Caractere inesperado {ch:?}.")));
            }
        };
        Ok(Some(kind))
    }
}

fn identifier_letter(ch: char) -> bool {
    ch.is_ascii_alphabetic() || "_áÁãÃâÂàÀéÉêÊíÍóÓõÕôÔúÚçÇ".contains(ch)
}
