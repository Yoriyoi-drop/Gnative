//! Token & jenis token bahasa `.gn`.

use crate::span::Span;

/// Jenis token. Subset TypeScript yang relevan untuk GUI.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literal
    Ident(String),
    Number(f64),
    String(String),
    Template(String),
    True,
    False,
    Null,

    // Kata kunci
    Use,
    Interface,
    Type,
    Component,
    Window,
    Function,
    Const,
    Let,
    /// `state { ... }` — blok state internal komponen.
    State,
    Return,
    If,
    Else,
    For,
    Of,
    In,
    Extends,
    Render,
    Element,
    As,
    New,
    Public,
    Private,
    Readonly,

    // Simbol
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Lt,      // < (juga membuka elemen/generic)
    LtEq,    // <=
    Gt,      // >
    GtEq,    // >=
    LtSlash, // </
    SlashGt, // />
    Comma,
    Dot,
    DotDotDot,
    Semi,
    Colon,
    Question,
    Arrow,   // =>
    Eq,      // =
    PlusEq,  // +=
    MinusEq, // -=
    EqEq,    // ==
    NotEq,   // !=
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    Pipe,
    AmpAmp,
    PipePipe,
    Bang,
    At,

    Eof,
}

impl TokenKind {
    /// Deskripsi pendek untuk pesan error.
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Ident(name) => format!("identifier `{name}`"),
            TokenKind::Number(n) => format!("number `{n}`"),
            TokenKind::String(s) => format!("string `{s}`"),
            TokenKind::Eof => "end of file".to_string(),
            other => format!("`{other:?}`"),
        }
    }
}

/// Satu token beserta posisinya.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn is(&self, kind: &TokenKind) -> bool {
        &self.kind == kind
    }
}
