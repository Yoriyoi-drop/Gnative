//! Parser: token → AST (recursive descent).

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::token::{Token, TokenKind};
use crate::ty::Type;

/// Parser recursive descent untuk bahasa `.gn`.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    pub diagnostics: Vec<Diagnostic>,
    /// Sentinel `Eof` milik parser untuk fallback tanpa panic.
    /// Dipakai bila `tokens` kosong (hanya via API langsung/fuzz) agar
    /// `current`/`prev`/`bump` selalu punya referensi valid.
    eof: Token,
}

impl Parser {
    /// Buat parser dari token. Menjamin sentinel `Eof` di akhir.
    pub fn new(mut tokens: Vec<Token>) -> Self {
        // Jaminan defensif: selalu ada sentinel `Eof` di akhir walau lexer
        // dipanggil dengan cara tak standar (mis. vektor kosong dari test/fuzz).
        // Cek elemen TERAKHIR (bukan `any`) agar `Eof` tengah tidak menipu.
        let need_eof = !tokens.last().is_some_and(|t| t.is(&TokenKind::Eof));
        if need_eof {
            tokens.push(Token::new(
                TokenKind::Eof,
                crate::span::Span::default(),
            ));
        }
        Self {
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
            eof: Token::new(TokenKind::Eof, crate::span::Span::default()),
        }
    }

    pub fn parse_module(&mut self) -> Module {
        let mut module = Module {
            name: "main".into(),
            imports: Vec::new(),
            decls: Vec::new(),
        };

        while !self.at_eof() {
            if self.check(&TokenKind::Use) {
                self.bump();
                if let Some(TokenKind::String(path)) = self.eat_kind() {
                    module.imports.push(Import {
                        path,
                        span: self.prev().span,
                    });
                }
                self.eat(&TokenKind::Semi);
            } else {
                match self.parse_decl() {
                    Some(decl) => module.decls.push(decl),
                    None => {
                        // Error recovery: lewati satu token.
                        self.error_here("tidak bisa mengurai deklarasi");
                        self.bump();
                    }
                }
            }
        }
        module
    }

    fn parse_decl(&mut self) -> Option<Decl> {
        if self.check(&TokenKind::Component) {
            self.parse_component().map(Decl::Component)
        } else if self.check(&TokenKind::Window) {
            self.parse_window().map(Decl::Window)
        } else if self.check(&TokenKind::Interface) {
            self.parse_interface().map(Decl::Interface)
        } else if self.check(&TokenKind::Type) {
            self.parse_type_alias().map(Decl::TypeAlias)
        } else if self.check(&TokenKind::Function) {
            self.parse_function().map(Decl::Function)
        } else if self.check(&TokenKind::Const) {
            self.parse_const().map(Decl::Const)
        } else {
            None
        }
    }

    fn parse_component(&mut self) -> Option<ComponentDecl> {
        let start = self.bump().span; // `component`
        let name = self.expect_ident("nama komponen")?;
        let mut props_type = None;
        if self.check(&TokenKind::LParen) {
            self.bump();
            // `(props: PropsType)`
            if let Some(TokenKind::Ident(_)) = self.peek_kind() {
                self.bump(); // nama param
                self.eat(&TokenKind::Colon);
                if let Some(TokenKind::Ident(t)) = self.peek_kind().cloned() {
                    props_type = Some(t);
                    self.bump();
                }
            }
            self.eat(&TokenKind::RParen);
        }
        self.eat(&TokenKind::LBrace);
        let state = self.parse_state_block();
        let (body, methods) = self.parse_block_members();
        self.eat(&TokenKind::RBrace);
        Some(ComponentDecl {
            name,
            props_type,
            state,
            body,
            methods,
            span: start.join(self.prev().span),
        })
    }

    /// `state { count: number = 0; }` — blok state internal komponen.
    fn parse_state_block(&mut self) -> Vec<StateField> {
        let mut fields = Vec::new();
        if !self.check(&TokenKind::State) {
            return fields;
        }
        self.bump();
        self.eat(&TokenKind::LBrace);
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            let start = self.current().span;
            if let Some(TokenKind::Ident(name)) = self.peek_kind().cloned() {
                self.bump();
                self.eat(&TokenKind::Colon);
                let ty = self.parse_type();
                let init = if self.eat(&TokenKind::Eq).is_some() {
                    Some(self.parse_expr())
                } else {
                    None
                };
                fields.push(StateField {
                    name,
                    ty,
                    init,
                    span: start.join(self.prev().span),
                });
            } else {
                self.error_here("field state harus `nama: Tipe = nilai;`");
                self.bump();
            }
            self.eat(&TokenKind::Semi);
        }
        self.eat(&TokenKind::RBrace);
        fields
    }

    fn parse_window(&mut self) -> Option<WindowDecl> {
        let start = self.bump().span; // `window`
        let name = self.expect_ident("nama window")?;
        self.eat(&TokenKind::LBrace);
        let mut props = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            let prop_start = self.current().span;
            if let Some(TokenKind::Ident(key)) = self.peek_kind().cloned() {
                self.bump();
                self.eat(&TokenKind::Colon);
                let value = self.parse_expr();
                props.push(PropLiteral {
                    name: key,
                    value,
                    span: prop_start.join(self.prev().span),
                });
            } else {
                self.error_here("properti window harus `nama: nilai`");
                self.bump();
            }
            self.eat(&TokenKind::Semi);
        }
        self.eat(&TokenKind::RBrace);
        Some(WindowDecl {
            name,
            props,
            span: start.join(self.prev().span),
        })
    }

    fn parse_interface(&mut self) -> Option<InterfaceDecl> {
        let start = self.bump().span; // `interface`
        let name = self.expect_ident("nama interface")?;
        let mut generics = Vec::new();
        if self.check(&TokenKind::Lt) {
            self.bump();
            while !self.check(&TokenKind::Gt) && !self.at_eof() {
                if let Some(TokenKind::Ident(g)) = self.peek_kind().cloned() {
                    generics.push(g);
                    self.bump();
                }
                self.eat(&TokenKind::Comma);
            }
            self.eat(&TokenKind::Gt);
        }
        self.eat(&TokenKind::LBrace);
        let fields = self.parse_fields();
        self.eat(&TokenKind::RBrace);
        Some(InterfaceDecl {
            name,
            generics,
            fields,
            span: start.join(self.prev().span),
        })
    }

    fn parse_fields(&mut self) -> Vec<FieldDecl> {
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            let start = self.current().span;
            if let Some(TokenKind::Ident(name)) = self.peek_kind().cloned() {
                self.bump();
                let optional = self.eat(&TokenKind::Question).is_some();
                self.eat(&TokenKind::Colon);
                let ty = self.parse_type();
                fields.push(FieldDecl {
                    name,
                    ty,
                    optional,
                    span: start.join(self.prev().span),
                });
            } else {
                self.error_here("field harus `nama: Tipe`");
                self.bump();
            }
            self.eat(&TokenKind::Semi);
        }
        fields
    }

    fn parse_type_alias(&mut self) -> Option<TypeAliasDecl> {
        let start = self.bump().span; // `type`
        let name = self.expect_ident("nama tipe")?;
        self.eat(&TokenKind::Eq);
        let ty = self.parse_type();
        self.eat(&TokenKind::Semi);
        Some(TypeAliasDecl {
            name,
            ty,
            span: start.join(self.prev().span),
        })
    }

    fn parse_type(&mut self) -> Type {
        self.parse_union_type()
    }

    fn parse_union_type(&mut self) -> Type {
        let mut parts = vec![self.parse_atomic_type()];
        while self.eat(&TokenKind::Pipe).is_some() {
            parts.push(self.parse_atomic_type());
        }
        if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Type::Union(parts)
        }
    }

    fn parse_atomic_type(&mut self) -> Type {
        // Ident: primitif (`string`), named, atau generic `Name<T, U>`.
        let base = if let Some(TokenKind::Ident(name)) = self.peek_kind().cloned() {
            self.bump();
            match name.as_str() {
                "string" => Type::String,
                "number" => Type::Number,
                "boolean" => Type::Boolean,
                "void" => Type::Void,
                "Element" => Type::Element,
                _ if self.check(&TokenKind::Lt) => {
                    self.bump();
                    let mut args = Vec::new();
                    while !self.check(&TokenKind::Gt) && !self.at_eof() {
                        args.push(self.parse_type());
                        self.eat(&TokenKind::Comma);
                    }
                    self.eat(&TokenKind::Gt);
                    Type::Generic { name, args }
                }
                _ => Type::Named(name),
            }
        } else {
            match self.peek_kind().cloned() {
                Some(TokenKind::String(s)) => {
                    self.bump();
                    Type::StringLit(s)
                }
                Some(TokenKind::Number(n)) => {
                    self.bump();
                    Type::NumberLit(n)
                }
                Some(TokenKind::Null) => {
                    self.bump();
                    Type::Null
                }
                Some(TokenKind::LParen) => {
                    self.bump();
                    let mut params = Vec::new();
                    while !self.check(&TokenKind::RParen) && !self.at_eof() {
                        // (a: T, b: T)
                        if let Some(TokenKind::Ident(_)) = self.peek_kind() {
                            self.bump();
                            self.eat(&TokenKind::Colon);
                        }
                        params.push(self.parse_atomic_type());
                        self.eat(&TokenKind::Comma);
                    }
                    self.eat(&TokenKind::RParen);
                    self.eat(&TokenKind::Arrow);
                    let ret = self.parse_type();
                    Type::Fn {
                        params,
                        ret: Box::new(ret),
                    }
                }
                Some(TokenKind::Element) => {
                    self.bump();
                    Type::Element
                }
                Some(TokenKind::LBracket) => {
                    self.bump();
                    let inner = self.parse_type();
                    self.eat(&TokenKind::RBracket);
                    Type::Array(Box::new(inner))
                }
                _ => {
                    self.error_here("tipe tidak valid");
                    Type::Unknown
                }
            }
        };
        // Postfix: T[] (diterapkan pada semua bentuk dasar).
        if self.check(&TokenKind::LBracket) {
            self.bump();
            self.eat(&TokenKind::RBracket);
            Type::Array(Box::new(base))
        } else {
            base
        }
    }

    fn parse_function(&mut self) -> Option<FunctionDecl> {
        let start = self.bump().span; // `function`
        let name = self.expect_ident("nama fungsi")?;
        let (params, ret) = self.parse_params_and_ret();
        self.eat(&TokenKind::LBrace);
        let (body, _) = self.parse_block_members();
        self.eat(&TokenKind::RBrace);
        Some(FunctionDecl {
            name,
            params,
            ret,
            body,
            span: start.join(self.prev().span),
        })
    }

    fn parse_const(&mut self) -> Option<ConstDecl> {
        let start = self.bump().span; // `const`
        let name = self.expect_ident("nama konstanta")?;
        self.eat(&TokenKind::Eq);
        let value = self.parse_expr();
        self.eat(&TokenKind::Semi);
        Some(ConstDecl {
            name,
            value,
            span: start.join(self.prev().span),
        })
    }

    fn parse_params_and_ret(&mut self) -> (Vec<Param>, Option<Type>) {
        self.eat(&TokenKind::LParen);
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.at_eof() {
            let start = self.current().span;
            let name = match self.peek_kind().cloned() {
                Some(TokenKind::Ident(n)) => {
                    self.bump();
                    n
                }
                _ => {
                    self.error_here("nama parameter tidak valid");
                    self.bump();
                    "_".into()
                }
            };
            let ty = if self.eat(&TokenKind::Colon).is_some() {
                Some(self.parse_type())
            } else {
                None
            };
            params.push(Param {
                name,
                ty,
                span: start.join(self.prev().span),
            });
            self.eat(&TokenKind::Comma);
        }
        self.eat(&TokenKind::RParen);
        let ret = if self.eat(&TokenKind::Colon).is_some() {
            Some(self.parse_type())
        } else {
            None
        };
        (params, ret)
    }

    /// Parse isi blok `{ ... }`: pisahkan method (`render() {...}`) dari stmt.
    fn parse_block_members(&mut self) -> (Vec<Stmt>, Vec<MethodDecl>) {
        let mut stmts = Vec::new();
        let mut methods = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            let is_ident = matches!(
                self.peek_kind(),
                Some(TokenKind::Render) | Some(TokenKind::Ident(_))
            );
            if is_ident {
                // Method? `render(): Element {` atau `foo(a: T): R {`
                let save = self.pos;
                let start = self.current().span;
                let name = match self.peek_kind().cloned() {
                    Some(TokenKind::Render) => {
                        self.bump();
                        "render".into()
                    }
                    Some(TokenKind::Ident(n)) => {
                        self.bump();
                        n
                    }
                    _ => unreachable!(),
                };
                if self.check(&TokenKind::LParen) {
                    let (params, ret) = self.parse_params_and_ret();
                    if self.check(&TokenKind::LBrace) {
                        self.bump();
                        let (body, _) = self.parse_block_members();
                        self.eat(&TokenKind::RBrace);
                        methods.push(MethodDecl {
                            name,
                            params,
                            ret,
                            body,
                            span: start.join(self.prev().span),
                        });
                        continue;
                    }
                }
                // Bukan method → rollback ke statement.
                self.pos = save;
            }
            match self.parse_stmt() {
                Some(stmt) => stmts.push(stmt),
                None => {
                    self.error_here("pernyataan tidak valid");
                    self.bump();
                }
            }
        }
        (stmts, methods)
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        if self.check(&TokenKind::Return) {
            let start = self.bump().span;
            let expr = if self.check(&TokenKind::Semi) {
                Expr::Null(start)
            } else {
                self.parse_expr()
            };
            self.eat(&TokenKind::Semi);
            return Some(Stmt::Return(expr, start.join(self.prev().span)));
        }
        if self.check(&TokenKind::Let) {
            let start = self.bump().span;
            let name = self.expect_ident("nama variabel")?;
            let ty = if self.eat(&TokenKind::Colon).is_some() {
                Some(self.parse_type())
            } else {
                None
            };
            let init = if self.eat(&TokenKind::Eq).is_some() {
                Some(self.parse_expr())
            } else {
                None
            };
            self.eat(&TokenKind::Semi);
            return Some(Stmt::Let {
                name,
                ty,
                init,
                span: start.join(self.prev().span),
            });
        }
        if self.check(&TokenKind::If) {
            let start = self.bump().span;
            self.eat(&TokenKind::LParen);
            let cond = self.parse_expr();
            self.eat(&TokenKind::RParen);
            self.eat(&TokenKind::LBrace);
            let then_branch = self.parse_stmts_until_rbrace();
            self.eat(&TokenKind::RBrace);
            let mut else_branch = Vec::new();
            if self.eat(&TokenKind::Else).is_some() {
                if self.eat(&TokenKind::If).is_some() {
                    // else-if: parse sebagai If bersarang.
                    self.pos = self.pos.saturating_sub(1);
                }
                self.eat(&TokenKind::LBrace);
                else_branch = self.parse_stmts_until_rbrace();
                self.eat(&TokenKind::RBrace);
            }
            return Some(Stmt::If {
                cond,
                then_branch,
                else_branch,
                span: start.join(self.prev().span),
            });
        }
        if self.check(&TokenKind::For) {
            let start = self.bump().span;
            self.eat(&TokenKind::LParen);
            let var = self.expect_ident("variabel loop")?;
            self.eat(&TokenKind::Of);
            let iter = self.parse_expr();
            self.eat(&TokenKind::RParen);
            self.eat(&TokenKind::LBrace);
            let body = self.parse_stmts_until_rbrace();
            self.eat(&TokenKind::RBrace);
            return Some(Stmt::For {
                var,
                iter,
                body,
                span: start.join(self.prev().span),
            });
        }
        let expr = self.parse_expr();
        self.eat(&TokenKind::Semi);
        Some(Stmt::ExprStmt(expr, self.prev().span))
    }

    fn parse_stmts_until_rbrace(&mut self) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.at_eof() {
            match self.parse_stmt() {
                Some(s) => stmts.push(s),
                None => {
                    self.error_here("pernyataan tidak valid");
                    self.bump();
                }
            }
        }
        stmts
    }

    fn parse_expr(&mut self) -> Expr {
        self.parse_assign()
    }

    /// Assignment `x = v` / `x += v` / `x -= v` (right-assoc, precedence
    /// terendah) — dasar mutasi state di body method.
    fn parse_assign(&mut self) -> Expr {
        let left = self.parse_binary(0);
        let op = match self.peek_kind() {
            Some(TokenKind::Eq) => Some(AssignOp::Assign),
            Some(TokenKind::PlusEq) => Some(AssignOp::Add),
            Some(TokenKind::MinusEq) => Some(AssignOp::Sub),
            _ => None,
        };
        match op {
            Some(op) => {
                let span = self.bump().span;
                let value = self.parse_assign();
                let left_span = left.span();
                Expr::Assign {
                    target: Box::new(left),
                    op,
                    value: Box::new(value),
                    span: left_span.join(span),
                }
            }
            None => left,
        }
    }

    fn parse_binary(&mut self, min_prec: u8) -> Expr {
        let mut left = self.parse_unary();
        loop {
            let (op, prec) = match self.peek_kind() {
                Some(TokenKind::AmpAmp) => (BinOp::And, 3),
                Some(TokenKind::PipePipe) => (BinOp::Or, 2),
                Some(TokenKind::EqEq) => (BinOp::EqEq, 4),
                Some(TokenKind::NotEq) => (BinOp::NotEq, 4),
                // `<` sebagai operator hanya bila left bukan elemen — kalau
                // left elemen, `<` berikutnya adalah elemen child baru.
                Some(TokenKind::Lt) if !matches!(left, Expr::Element(..)) => (BinOp::Lt, 4),
                Some(TokenKind::LtEq) => (BinOp::Le, 4),
                Some(TokenKind::Gt) => (BinOp::Gt, 4),
                Some(TokenKind::GtEq) => (BinOp::Ge, 4),
                Some(TokenKind::Plus) => (BinOp::Add, 5),
                Some(TokenKind::Minus) => (BinOp::Sub, 5),
                Some(TokenKind::Star) => (BinOp::Mul, 6),
                Some(TokenKind::Slash) => (BinOp::Div, 6),
                _ => break,
            };
            if prec < min_prec {
                break;
            }
            let span = self.bump().span;
            let left_span = left.span();
            let right = self.parse_binary(prec + 1);
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
                span: left_span.join(span),
            };
        }
        left
    }

    fn parse_unary(&mut self) -> Expr {
        if self.check(&TokenKind::Bang) {
            let span = self.bump().span;
            let operand = self.parse_unary();
            return Expr::UnOp {
                op: UnOp::Not,
                operand: Box::new(operand),
                span,
            };
        }
        if self.check(&TokenKind::Minus) {
            let span = self.bump().span;
            let operand = self.parse_unary();
            return Expr::UnOp {
                op: UnOp::Neg,
                operand: Box::new(operand),
                span,
            };
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_atom();
        loop {
            if self.eat(&TokenKind::Dot).is_some() {
                if let Some(TokenKind::Ident(field)) = self.peek_kind().cloned() {
                    let span = expr.span();
                    self.bump();
                    expr = Expr::Member {
                        object: Box::new(expr),
                        field,
                        span,
                    };
                } else {
                    break;
                }
            } else if self.check(&TokenKind::LParen) {
                let span = expr.span();
                self.bump();
                let mut args = Vec::new();
                while !self.check(&TokenKind::RParen) && !self.at_eof() {
                    args.push(self.parse_expr());
                    self.eat(&TokenKind::Comma);
                }
                self.eat(&TokenKind::RParen);
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
            } else {
                break;
            }
        }
        expr
    }

    fn parse_atom(&mut self) -> Expr {
        let span = self.current().span;
        match self.peek_kind().cloned() {
            Some(TokenKind::Number(n)) => {
                self.bump();
                Expr::Number(n, span)
            }
            Some(TokenKind::String(s)) => {
                self.bump();
                Expr::String(s, span)
            }
            Some(TokenKind::Template(s)) => {
                self.bump();
                Expr::Template(s, span)
            }
            Some(TokenKind::True) => {
                self.bump();
                Expr::Bool(true, span)
            }
            Some(TokenKind::False) => {
                self.bump();
                Expr::Bool(false, span)
            }
            Some(TokenKind::Null) => {
                self.bump();
                Expr::Null(span)
            }
            Some(TokenKind::Ident(name)) => {
                self.bump();
                Expr::Ident(name, span)
            }
            Some(TokenKind::LParen) => {
                self.bump();
                // Arrow function `(x: number) => ...`
                let save = self.pos;
                let mut params = Vec::new();
                let mut is_arrow = false;
                while !self.check(&TokenKind::RParen) && !self.at_eof() {
                    if let Some(TokenKind::Ident(p)) = self.peek_kind().cloned() {
                        self.bump();
                        let ty = if self.eat(&TokenKind::Colon).is_some() {
                            Some(self.parse_type())
                        } else {
                            None
                        };
                        params.push(Param {
                            name: p,
                            ty,
                            span: self.prev().span,
                        });
                    }
                    self.eat(&TokenKind::Comma);
                }
                self.eat(&TokenKind::RParen);
                if self.check(&TokenKind::Arrow) {
                    self.bump();
                    is_arrow = true;
                }
                if is_arrow {
                    let body = self.parse_expr();
                    Expr::Arrow {
                        params,
                        body: Box::new(body),
                        span,
                    }
                } else {
                    // Bukan arrow → rollback dan parse sebagai paren biasa.
                    self.pos = save;
                    let inner = self.parse_expr();
                    self.eat(&TokenKind::RParen);
                    inner
                }
            }
            Some(TokenKind::LBracket) => {
                self.bump();
                let mut items = Vec::new();
                while !self.check(&TokenKind::RBracket) && !self.at_eof() {
                    items.push(self.parse_expr());
                    self.eat(&TokenKind::Comma);
                }
                self.eat(&TokenKind::RBracket);
                Expr::Array(items, span)
            }
            Some(TokenKind::LBrace) => {
                self.bump();
                let mut props = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.at_eof() {
                    let pstart = self.current().span;
                    if let Some(TokenKind::Ident(key)) = self.peek_kind().cloned() {
                        self.bump();
                        self.eat(&TokenKind::Colon);
                        let value = self.parse_expr();
                        props.push(PropLiteral {
                            name: key,
                            value,
                            span: pstart.join(self.prev().span),
                        });
                    } else {
                        // Robustness: jangan pernah macet pada isi yang
                        // bukan `key: value` — lompati token dan lanjut.
                        self.error_here("objek literal harus berisi `key: value`");
                        self.bump();
                    }
                    self.eat(&TokenKind::Comma);
                }
                self.eat(&TokenKind::RBrace);
                Expr::Object(props, span)
            }
            Some(TokenKind::Lt) => {
                let el = self.parse_element();
                Expr::Element(el, span)
            }
            _ => {
                self.error_here("ekspresi tidak valid");
                self.bump();
                Expr::Null(span)
            }
        }
    }

    fn parse_element(&mut self) -> Element {
        let start = self.bump().span; // `<`
        let tag = self.expect_ident("nama elemen").unwrap_or_default();
        let mut attrs = Vec::new();
        while !self.check(&TokenKind::Gt) && !self.check(&TokenKind::SlashGt) && !self.at_eof() {
            let astart = self.current().span;
            let name = self.expect_ident("nama atribut").unwrap_or_default();
            let value = if self.eat(&TokenKind::Eq).is_some() {
                if self.check(&TokenKind::LBrace) {
                    self.bump();
                    let v = self.parse_expr();
                    self.eat(&TokenKind::RBrace);
                    Some(v)
                } else if let Some(TokenKind::String(s)) = self.peek_kind().cloned() {
                    self.bump();
                    Some(Expr::String(s, self.prev().span))
                } else {
                    None
                }
            } else {
                None
            };
            attrs.push(Attr {
                name,
                value,
                span: astart.join(self.prev().span),
            });
        }
        let mut children = Vec::new();
        let end = if self.eat(&TokenKind::SlashGt).is_some() {
            self.prev().span
        } else {
            self.eat(&TokenKind::Gt);
            // Anak elemen.
            while !self.check(&TokenKind::LtSlash) && !self.at_eof() {
                children.push(self.parse_expr());
            }
            self.eat(&TokenKind::LtSlash);
            self.expect_ident("nama elemen penutup");
            self.eat(&TokenKind::Gt);
            self.prev().span
        };
        Element {
            tag,
            attrs,
            children,
            span: start.join(end),
        }
    }

    // --- Utilitas token ---

    /// Token saat ini; jatuh ke sentinel `Eof` milik parser bila `pos`
    /// di ujung. Tidak pernah panic walau `pos` mencapai akhir atau
    /// `tokens` kosong (input terpotong / API langsung).
    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&self.eof)
    }

    fn prev(&self) -> &Token {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .unwrap_or(&self.eof)
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn bump(&mut self) -> &Token {
        // Jangan melangkah melewati sentinel `Eof`: bila sudah di akhir,
        // jepit `pos` ke indeks `Eof` dan kembalikan `Eof` (idempoten,
        // anti-panic input terpotong). `saturating_add` cegah overflow teoritis.
        if self.pos.saturating_add(1) < self.tokens.len() {
            let idx = self.pos;
            self.pos += 1;
            &self.tokens[idx]
        } else if self.tokens.is_empty() {
            &self.eof
        } else {
            self.pos = self.tokens.len().saturating_sub(1);
            &self.tokens[self.pos]
        }
    }

    fn at_eof(&self) -> bool {
        self.tokens.get(self.pos).is_some_and(|t| t.is(&TokenKind::Eof))
            || self.pos >= self.tokens.len()
    }

    fn check(&self, kind: &TokenKind) -> bool {
        self.tokens.get(self.pos).is_some_and(|t| t.is(kind))
    }

    fn eat(&mut self, kind: &TokenKind) -> Option<&Token> {
        if self.check(kind) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn eat_kind(&mut self) -> Option<TokenKind> {
        let kind = self.peek_kind()?.clone();
        self.bump();
        Some(kind)
    }

    fn expect_ident(&mut self, what: &str) -> Option<String> {
        match self.peek_kind().cloned() {
            Some(TokenKind::Ident(name)) => {
                self.bump();
                Some(name)
            }
            _ => {
                self.error_here(&format!("diharapkan {what}"));
                None
            }
        }
    }

    fn error_here(&mut self, message: &str) {
        let span = self.current().span;
        self.diagnostics
            .push(Diagnostic::error(span, message.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Module {
        let tokens = crate::lexer::Lexer::new(src).tokenize();
        let mut parser = Parser::new(tokens);
        parser.parse_module()
    }

    #[test]
    fn parse_component_window() {
        let m = parse(
            "use \"@gnative/ui\";\ncomponent Button(props: Props) {\n  render(): Element {\n    return <button label={props.label} />;\n  }\n}\nwindow App { title: \"x\"; size: [800, 600]; }",
        );
        assert_eq!(m.imports.len(), 1);
        assert_eq!(m.decls.len(), 2);
    }

    #[test]
    fn parse_interface() {
        let m = parse("interface Props { label: string; kind: \"primary\" | \"secondary\"; }");
        match &m.decls[0] {
            Decl::Interface(i) => {
                assert_eq!(i.name, "Props");
                assert_eq!(i.fields.len(), 2);
                assert!(matches!(&i.fields[1].ty, Type::Union(_)));
            }
            _ => panic!("harusnya interface"),
        }
    }

    #[test]
    fn parse_state_block_and_assign() {
        let m = parse(
            "component C {\n\
               state { count: number = 0; }\n\
               render(): Element { return <text/>; }\n\
               inc(): void { count += 1; }\n\
             }",
        );
        match &m.decls[0] {
            Decl::Component(c) => {
                assert_eq!(c.state.len(), 1);
                assert_eq!(c.state[0].name, "count");
                assert!(matches!(c.state[0].ty, Type::Number));
                // `render` + `inc` sebagai method.
                assert_eq!(c.methods.len(), 2);
                let inc = &c.methods[1];
                assert!(matches!(
                    &inc.body[0],
                    Stmt::ExprStmt(
                        Expr::Assign {
                            op: AssignOp::Add,
                            ..
                        },
                        _
                    )
                ));
            }
            _ => panic!("harusnya komponen"),
        }
    }

    #[test]
    fn parse_primitive_types() {
        // Regresi: `string`/`number`/`boolean` harus jadi primitif,
        // bukan `Type::Named("string")` (yang dulu menghasilkan kode
        // Rust `string` lowercase dan gagal kompilasi).
        let m = parse("interface P { label: string; count: number; ok: boolean; }");
        match &m.decls[0] {
            Decl::Interface(i) => {
                assert!(matches!(i.fields[0].ty, Type::String));
                assert!(matches!(i.fields[1].ty, Type::Number));
                assert!(matches!(i.fields[2].ty, Type::Boolean));
            }
            _ => panic!("harusnya interface"),
        }
    }

    /// Regresi P0-01: input kosong/terpotong tidak boleh panic.
    #[test]
    fn parse_tidak_panic_pada_input_kosong_dan_terpotong() {
        for src in [
            "",
            "   ",
            "// komentar saja",
            "window",
            "window App {",
            "window App { title:",
            "component",
            "component C {",
            "(((((",
            "window App { title: \"x\";",
        ] {
            let tokens = crate::lexer::Lexer::new(src).tokenize();
            let mut parser = Parser::new(tokens);
            let _ = parser.parse_module();
            // Tidak panic = lolos; diagnostik boleh ada.
        }
    }

    /// Regresi P0-01: vektor token kosong (pemakaian API langsung) tetap aman.
    #[test]
    fn parse_token_kosong_tetap_aman() {
        let mut parser = Parser::new(Vec::new());
        let m = parser.parse_module();
        assert!(m.decls.is_empty());
    }
}
