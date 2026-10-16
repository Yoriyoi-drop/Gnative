//! AST bahasa `.gn`.

use crate::span::Span;
use crate::ty::Type;

/// Satu file `.gn` = satu modul.
#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub imports: Vec<Import>,
    pub decls: Vec<Decl>,
}

/// Deklarasi `use "@gnative/ui";`.
#[derive(Debug, Clone)]
pub struct Import {
    pub path: String,
    pub span: Span,
}

/// Deklarasi tingkat atas.
#[derive(Debug, Clone)]
pub enum Decl {
    Component(ComponentDecl),
    Window(WindowDecl),
    Interface(InterfaceDecl),
    TypeAlias(TypeAliasDecl),
    Function(FunctionDecl),
    Const(ConstDecl),
}

/// `interface Name { prop: Type; ... }`.
#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
}

/// `type Name = ...;`
#[derive(Debug, Clone)]
pub struct TypeAliasDecl {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

/// `component Name(props: Props) { ... }`
#[derive(Debug, Clone)]
pub struct ComponentDecl {
    pub name: String,
    pub props_type: Option<String>,
    /// State internal: `state { count: number = 0; }`
    pub state: Vec<StateField>,
    pub body: Vec<Stmt>,
    pub methods: Vec<MethodDecl>,
    pub span: Span,
}

/// Satu field state: `count: number = 0;`
#[derive(Debug, Clone)]
pub struct StateField {
    pub name: String,
    pub ty: Type,
    pub init: Option<Expr>,
    pub span: Span,
}

/// `window Name { ... }`
#[derive(Debug, Clone)]
pub struct WindowDecl {
    pub name: String,
    pub props: Vec<PropLiteral>,
    pub span: Span,
}

/// Properti dalam window/object literal.
#[derive(Debug, Clone)]
pub struct PropLiteral {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

/// `function name(a: T): R { ... }`
#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// `const name = ...;`
#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

/// `render(): Element { ... }` dan method lain.
#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// Field dalam interface.
#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Type,
    pub optional: bool,
    pub span: Span,
}

/// Parameter fungsi/method.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
    pub span: Span,
}

/// Pernyataan.
#[derive(Debug, Clone)]
pub enum Stmt {
    Return(Expr, Span),
    ExprStmt(Expr, Span),
    If {
        cond: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Vec<Stmt>,
        span: Span,
    },
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Let {
        name: String,
        ty: Option<Type>,
        init: Option<Expr>,
        span: Span,
    },
}

/// Ekspresi.
#[derive(Debug, Clone)]
pub enum Expr {
    Number(f64, Span),
    String(String, Span),
    Template(String, Span),
    Bool(bool, Span),
    Null(Span),
    Ident(String, Span),
    Member {
        object: Box<Expr>,
        field: String,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
        span: Span,
    },
    UnOp {
        op: UnOp,
        operand: Box<Expr>,
        span: Span,
    },
    Assign {
        target: Box<Expr>,
        op: AssignOp,
        value: Box<Expr>,
        span: Span,
    },
    Arrow {
        params: Vec<Param>,
        body: Box<Expr>,
        span: Span,
    },
    Array(Vec<Expr>, Span),
    Object(Vec<PropLiteral>, Span),
    /// Elemen deklaratif `<Button label={x} />`.
    Element(Element, Span),
}

/// Elemen deklaratif (JSX-like).
#[derive(Debug, Clone)]
pub struct Element {
    pub tag: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Expr>,
    pub span: Span,
}

/// Atribut elemen: `label="x"` atau `label={expr}`.
#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    /// Sisa bagi (`%`).
    Mod,
    EqEq,
    NotEq,
    /// Perbandingan relasional.
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

/// Operator assignment: `=` / `+=` / `-=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Number(_, s)
            | Expr::String(_, s)
            | Expr::Template(_, s)
            | Expr::Bool(_, s)
            | Expr::Null(s)
            | Expr::Ident(_, s)
            | Expr::Member { span: s, .. }
            | Expr::Call { span: s, .. }
            | Expr::BinOp { span: s, .. }
            | Expr::UnOp { span: s, .. }
            | Expr::Assign { span: s, .. }
            | Expr::Arrow { span: s, .. }
            | Expr::Array(_, s)
            | Expr::Object(_, s)
            | Expr::Element(_, s) => *s,
        }
    }
}

impl std::fmt::Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Number(n, _) => write!(f, "{n}"),
            Expr::String(s, _) => write!(f, "\"{s}\""),
            Expr::Template(s, _) => write!(f, "`{s}`"),
            Expr::Bool(b, _) => write!(f, "{b}"),
            Expr::Null(_) => write!(f, "null"),
            Expr::Ident(i, _) => write!(f, "{i}"),
            Expr::Member { object, field, .. } => write!(f, "{object}.{field}"),
            Expr::Call { callee, args, .. } => {
                let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{callee}({})", args.join(", "))
            }
            Expr::BinOp {
                left, op, right, ..
            } => {
                let op = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Mod => "%",
                    BinOp::EqEq => "==",
                    BinOp::NotEq => "!=",
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                write!(f, "{left} {op} {right}")
            }
            Expr::UnOp { op, operand, .. } => {
                let op = match op {
                    UnOp::Neg => "-",
                    UnOp::Not => "!",
                };
                write!(f, "{op}{operand}")
            }
            Expr::Assign {
                target, op, value, ..
            } => {
                let op = match op {
                    AssignOp::Assign => "=",
                    AssignOp::Add => "+=",
                    AssignOp::Sub => "-=",
                };
                write!(f, "{target} {op} {value}")
            }
            Expr::Arrow { params, body, .. } => {
                let params: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
                write!(f, "({}) => {body}", params.join(", "))
            }
            Expr::Array(items, _) => {
                let items: Vec<String> = items.iter().map(|i| i.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Expr::Object(props, _) => {
                let props: Vec<String> = props
                    .iter()
                    .map(|p| format!("{}: {}", p.name, p.value))
                    .collect();
                write!(f, "{{ {} }}", props.join(", "))
            }
            Expr::Element(el, _) => write!(f, "<{}/>", el.tag),
        }
    }
}
