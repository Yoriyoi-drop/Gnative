//! Lexer: teks `.gn` → `Vec<Token>`.

use crate::span::{LineColumn, Span};
use crate::token::{Token, TokenKind};

/// Lexer bahasa `.gn`.
pub struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
    _marker: std::marker::PhantomData<&'a str>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
            _marker: std::marker::PhantomData,
        }
    }

    /// Ubah seluruh source menjadi token (berakhir dengan `Eof`).
    pub fn tokenize(mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let start_loc = self.loc();
            let c = match self.peek() {
                Some(c) => c,
                None => {
                    tokens.push(Token::new(TokenKind::Eof, self.span(start_loc)));
                    break;
                }
            };

            let kind = if c.is_ascii_alphabetic() || c == '_' {
                self.lex_ident_or_keyword()
            } else if c.is_ascii_digit() {
                self.lex_number()
            } else {
                self.lex_symbol()
            };

            tokens.push(Token::new(kind, self.span(start_loc)));
        }
        tokens
    }

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        let word: String = self.chars[start..self.pos].iter().collect();
        match word.as_str() {
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            "use" => TokenKind::Use,
            "interface" => TokenKind::Interface,
            "type" => TokenKind::Type,
            "component" => TokenKind::Component,
            "window" => TokenKind::Window,
            "function" => TokenKind::Function,
            "const" => TokenKind::Const,
            "let" => TokenKind::Let,
            "state" => TokenKind::State,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "for" => TokenKind::For,
            "of" => TokenKind::Of,
            "in" => TokenKind::In,
            "extends" => TokenKind::Extends,
            "render" => TokenKind::Render,
            "Element" => TokenKind::Element,
            "as" => TokenKind::As,
            "new" => TokenKind::New,
            "public" => TokenKind::Public,
            "private" => TokenKind::Private,
            "readonly" => TokenKind::Readonly,
            _ => TokenKind::Ident(word),
        }
    }

    fn lex_number(&mut self) -> TokenKind {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' {
                self.bump();
            } else {
                break;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        let n = text.parse::<f64>().unwrap_or(0.0);
        TokenKind::Number(n)
    }

    fn lex_string(&mut self, quote: char) -> TokenKind {
        // Pembuka sudah dikonsumsi oleh lex_symbol.
        let start = self.pos;
        while let Some(c) = self.peek() {
            self.bump();
            if c == quote {
                break;
            }
        }
        let text: String = self.chars[start..self.pos.saturating_sub(1)]
            .iter()
            .collect();
        TokenKind::String(text)
    }

    fn lex_template(&mut self) -> TokenKind {
        // Backtick pembuka sudah dikonsumsi oleh lex_symbol; isi disimpan mentah
        // (ekspansi `${}` = roadmap).
        let start = self.pos;
        while let Some(c) = self.peek() {
            self.bump();
            if c == '`' {
                break;
            }
        }
        let text: String = self.chars[start..self.pos.saturating_sub(1)]
            .iter()
            .collect();
        TokenKind::Template(text)
    }

    fn lex_symbol(&mut self) -> TokenKind {
        let c = self.bump().unwrap();
        match c {
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semi,
            ':' => TokenKind::Colon,
            '?' => TokenKind::Question,
            '@' => TokenKind::At,
            '.' => {
                if self.peek() == Some('.') {
                    self.bump();
                    self.bump();
                    TokenKind::DotDotDot
                } else {
                    TokenKind::Dot
                }
            }
            '=' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::EqEq
                } else if self.peek() == Some('>') {
                    self.bump();
                    TokenKind::Arrow
                } else {
                    TokenKind::Eq
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::NotEq
                } else {
                    TokenKind::Bang
                }
            }
            '<' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::LtEq
                } else if self.peek() == Some('/') {
                    self.bump();
                    TokenKind::LtSlash
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }
            '/' => {
                if self.peek() == Some('>') {
                    self.bump();
                    TokenKind::SlashGt
                } else {
                    TokenKind::Slash
                }
            }
            '+' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::PlusEq
                } else {
                    TokenKind::Plus
                }
            }
            '-' => {
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::MinusEq
                } else {
                    TokenKind::Minus
                }
            }
            '*' => TokenKind::Star,
            '%' => TokenKind::Percent,
            '&' => {
                if self.peek() == Some('&') {
                    self.bump();
                    TokenKind::AmpAmp
                } else {
                    TokenKind::Amp
                }
            }
            '|' => {
                if self.peek() == Some('|') {
                    self.bump();
                    TokenKind::PipePipe
                } else {
                    TokenKind::Pipe
                }
            }
            '"' | '\'' => self.lex_string(c),
            '`' => self.lex_template(),
            other => TokenKind::Ident(other.to_string()),
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek_n(1) == Some('/') => {
                    while let Some(c) = self.peek() {
                        self.bump();
                        if c == '\n' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_n(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn loc(&self) -> LineColumn {
        LineColumn {
            line: self.line,
            column: self.column,
        }
    }

    fn span(&self, start: LineColumn) -> Span {
        Span::new(
            self.pos,
            self.pos,
            start,
            LineColumn {
                line: self.line,
                column: self.column,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_basic() {
        let src = "use \"@gnative/ui\";\ncomponent Button {}";
        let tokens = Lexer::new(src).tokenize();
        assert!(matches!(tokens[0].kind, TokenKind::Use));
        assert!(matches!(tokens[1].kind, TokenKind::String(_)));
        assert!(matches!(tokens[3].kind, TokenKind::Component));
        assert!(tokens.last().unwrap().is(&TokenKind::Eof));
    }

    #[test]
    fn lex_arrow_and_element() {
        let src = "const f = (x: number) => x * 2; <Button label={\"ok\"} />";
        let tokens = Lexer::new(src).tokenize();
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert!(kinds.contains(&TokenKind::Arrow));
        assert!(kinds.contains(&TokenKind::Lt));
        assert!(kinds.contains(&TokenKind::SlashGt));
    }

    #[test]
    fn lex_comparison_operators() {
        let src = "if (a < b && c <= d && e >= f && g > h) {}";
        let tokens = Lexer::new(src).tokenize();
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert!(kinds.contains(&TokenKind::Lt));
        assert!(kinds.contains(&TokenKind::LtEq));
        assert!(kinds.contains(&TokenKind::GtEq));
        assert!(kinds.contains(&TokenKind::Gt));
        assert!(kinds.contains(&TokenKind::AmpAmp));
    }

    #[test]
    fn string_content_kept_intact() {
        // Regresi: huruf pertama string tidak boleh termakan double-bump.
        let src = "window App { title: \"Halo Gnative\"; }";
        let tokens = Lexer::new(src).tokenize();
        assert!(tokens.iter().any(|t| matches!(
            &t.kind,
            TokenKind::String(s) if s == "Halo Gnative"
        )));
    }
}
