//! Lexer: teks `.gn` → `Vec<Token>`.

use crate::diagnostic::Diagnostic;
use crate::span::{LineColumn, Span};
use crate::token::{Token, TokenKind};

/// Lexer bahasa `.gn`.
pub struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    /// Offset byte saat ini dalam source asli (bukan indeks char).
    byte_pos: usize,
    line: usize,
    column: usize,
    _marker: std::marker::PhantomData<&'a str>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            chars: src.chars().collect(),
            pos: 0,
            byte_pos: 0,
            line: 1,
            column: 1,
            _marker: std::marker::PhantomData,
        }
    }

    /// Ubah seluruh source menjadi token (berakhir dengan `Eof`).
    /// Mengembalikan token + diagnostic leksikal (angka tak valid, string
    /// tak terterminasi, dll).
    pub fn tokenize(mut self) -> (Vec<Token>, Vec<Diagnostic>) {
        let mut tokens = Vec::new();
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let start_loc = self.loc();
            let start_byte = self.byte_pos;
            let c = match self.peek() {
                Some(c) => c,
                None => {
                    tokens.push(Token::new(TokenKind::Eof, self.span(start_loc, start_byte)));
                    break;
                }
            };

            let kind = if c.is_ascii_alphabetic() || c == '_' {
                self.lex_ident_or_keyword()
            } else if c.is_ascii_digit() {
                self.lex_number(&mut diagnostics)
            } else {
                self.lex_symbol(&mut diagnostics)
            };

            tokens.push(Token::new(kind, self.span(start_loc, start_byte)));
        }
        (tokens, diagnostics)
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

    fn lex_number(&mut self, diags: &mut Vec<Diagnostic>) -> TokenKind {
        let start = self.pos;
        let start_byte = self.byte_pos;
        let start_loc = self.loc();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' {
                self.bump();
            } else {
                break;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        match text.parse::<f64>() {
            Ok(n) => TokenKind::Number(n),
            Err(_) => {
                diags.push(Diagnostic::error(
                    Span::new(start_byte, self.byte_pos, start_loc, self.loc()),
                    format!("angka tidak valid: `{text}`"),
                ));
                TokenKind::Number(0.0)
            }
        }
    }

    fn lex_string(&mut self, quote: char, diags: &mut Vec<Diagnostic>) -> TokenKind {
        // Pembuka sudah dikonsumsi oleh lex_symbol.
        let start_byte = self.byte_pos;
        let start_loc = self.loc();
        let mut text = String::new();
        let mut terminated = false;
        while let Some(c) = self.peek() {
            self.bump();
            if c == '\\' {
                // Escape: ambil karakter berikutnya (atau akhiri bila di EOF).
                match self.peek() {
                    Some(esc) => {
                        self.bump();
                        match esc {
                            'n' => text.push('\n'),
                            't' => text.push('\t'),
                            'r' => text.push('\r'),
                            '"' => text.push('"'),
                            '\'' => text.push('\''),
                            '\\' => text.push('\\'),
                            '`' => text.push('`'),
                            '0' => text.push('\0'),
                            other => {
                                text.push('\\');
                                text.push(other);
                            }
                        }
                    }
                    None => break, // backslash di akhir input → tak terterminasi
                }
                continue;
            }
            if c == quote {
                terminated = true;
                break;
            }
            text.push(c);
        }
        if !terminated {
            diags.push(Diagnostic::error(
                Span::new(start_byte, self.byte_pos, start_loc, self.loc()),
                "string literal tak terterminasi (kurang tanda kutip penutup)".into(),
            ));
        }
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

    fn lex_symbol(&mut self, diags: &mut Vec<Diagnostic>) -> TokenKind {
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
            '"' | '\'' => self.lex_string(c, diags),
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
        self.byte_pos += c.len_utf8();
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

    fn span(&self, start: LineColumn, start_byte: usize) -> Span {
        // Offset byte (bukan indeks char) dengan lebar > 0; kolom menunjuk
        // posisi akhir token agar pesan error akurat (termasuk unicode).
        Span::new(
            start_byte,
            self.byte_pos,
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
        let (tokens, _lex_diags) = Lexer::new(src).tokenize();
        assert!(matches!(tokens[0].kind, TokenKind::Use));
        assert!(matches!(tokens[1].kind, TokenKind::String(_)));
        assert!(matches!(tokens[3].kind, TokenKind::Component));
        assert!(tokens.last().unwrap().is(&TokenKind::Eof));
    }

    #[test]
    fn lex_arrow_and_element() {
        let src = "const f = (x: number) => x * 2; <Button label={\"ok\"} />";
        let (tokens, _lex_diags) = Lexer::new(src).tokenize();
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert!(kinds.contains(&TokenKind::Arrow));
        assert!(kinds.contains(&TokenKind::Lt));
        assert!(kinds.contains(&TokenKind::SlashGt));
    }

    #[test]
    fn lex_comparison_operators() {
        let src = "if (a < b && c <= d && e >= f && g > h) {}";
        let (tokens, _lex_diags) = Lexer::new(src).tokenize();
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
        let (tokens, _lex_diags) = Lexer::new(src).tokenize();
        assert!(tokens.iter().any(|t| matches!(
            &t.kind,
            TokenKind::String(s) if s == "Halo Gnative"
        )));
    }

    // --- P1-01: span presisi byte + lebar > 0 (termasuk unicode) ---

    #[test]
    fn span_byte_dan_lebar_nonzero() {
        let src = "abc 123";
        let (tokens, _) = Lexer::new(src).tokenize();
        // token pertama "abc": byte 0..3, lebar 3
        let s = tokens[0].span;
        assert_eq!(s.start, 0);
        assert_eq!(s.end, 3);
        assert!(s.end > s.start, "token harus punya lebar > 0");
        // token kedua "123" setelah spasi: byte 4..7, lebar 3
        let s2 = tokens[1].span;
        assert_eq!(s2.start, 4);
        assert_eq!(s2.end, 7);
    }

    #[test]
    fn span_byte_unicode_tergeser() {
        // Karakter unicode (😀 = 4 byte UTF-8) harus menggeser offset byte,
        // bukan indeks char. 😀 bukan alphanumeric → masuk lex_symbol sebagai
        // ident 1-karakter (4 byte), lalu `x` ident terpisah.
        let src = "😀x";
        let (tokens, _) = Lexer::new(src).tokenize();
        // token pertama = Ident("😀") span byte 0..4
        assert_eq!(tokens[0].span.start, 0);
        assert_eq!(tokens[0].span.end, 4);
        // token kedua `x` dimulai di byte 4
        assert_eq!(tokens[1].span.start, 4);
        assert_eq!(tokens[1].span.end, 5);
    }

    // --- P1-03: angka invalid harus error, bukan 0.0 diam ---

    #[test]
    fn angka_invalid_melaporkan_error() {
        let (tokens, diags) = Lexer::new("let x = 1.2.3;").tokenize();
        assert!(
            !diags.is_empty(),
            "angka 1.2.3 harus menghasilkan diagnostic"
        );
        assert!(diags
            .iter()
            .any(|d| d.message.contains("angka tidak valid")));
        // token Number tetap ada (nilai fallback 0.0) agar parser tak panic
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Number(_))));
    }

    #[test]
    fn angka_invalid_multi_titik_error() {
        // `1.2.3.4` gagal parse f64 → harus error (bukan 0.0 diam).
        let (_tokens, diags) = Lexer::new("let x = 1.2.3.4;").tokenize();
        assert!(diags
            .iter()
            .any(|d| d.message.contains("angka tidak valid")));
    }

    // --- P1-04: string escape + unterminated error ---

    #[test]
    fn string_escape_ditangani() {
        let (tokens, _diags) = Lexer::new("\"a\\nb\\tc\"").tokenize();
        let s = tokens
            .iter()
            .find_map(|t| match &t.kind {
                TokenKind::String(s) => Some(s.clone()),
                _ => None,
            })
            .expect("token string ada");
        assert_eq!(s, "a\nb\tc");
    }

    #[test]
    fn string_tak_terterminasi_error() {
        let src = "let x = \"unterminated;";
        let (tokens, diags) = Lexer::new(src).tokenize();
        assert!(
            diags.iter().any(|d| d.message.contains("tak terterminasi")),
            "string tak terterminasi harus error"
        );
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::String(_))));
    }

    // --- P1-02 (lexer-side): delimiter tak perlu di sini; diuji di parser ---

    #[test]
    fn expect_hilang_delimiter_error() {
        // `window App` tanpa `{` → parser laporkan error, tidak diam.
        let (module, diags) = crate::parse_source("window App title: \"x\";");
        assert!(
            diags.iter().any(|d| d.message.contains("diharapkan")),
            "delimiter hilang harus error, diagnostics: {:?}",
            diags
        );
        let _ = module;
    }
}
