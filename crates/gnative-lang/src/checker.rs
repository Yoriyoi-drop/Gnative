//! Checker: validasi semantik & tipe ringan pada AST `.gn`.

use std::collections::HashSet;

use crate::ast::*;
use crate::diagnostic::Diagnostic;

/// Checker semantik. Saat ini fokus pada:
/// - nama deklarasi duplikat,
/// - referensi tipe yang tidak dikenal,
/// - tag elemen yang tidak dikenal,
/// - pemanggilan `props` pada komponen tanpa props_type.
pub struct Checker {
    type_names: HashSet<String>,
    component_names: HashSet<String>,
    function_names: HashSet<String>,
    /// interface → field (untuk validasi props komponen).
    interfaces: std::collections::HashMap<String, Vec<FieldDecl>>,
    /// komponen → nama tipe props.
    component_props: std::collections::HashMap<String, String>,
}

impl Checker {
    pub fn new() -> Self {
        // Tipe bawaan bahasa.
        let mut type_names = HashSet::new();
        for t in ["string", "number", "boolean", "void", "null", "Element"] {
            type_names.insert(t.to_string());
        }
        Self {
            type_names,
            component_names: HashSet::new(),
            function_names: HashSet::new(),
            interfaces: std::collections::HashMap::new(),
            component_props: std::collections::HashMap::new(),
        }
    }

    pub fn check(&self, module: &mut Module, out: &mut Vec<Diagnostic>) {
        // Pass 1: kumpulkan nama.
        let mut checker = Self::new();
        for decl in &module.decls {
            match decl {
                Decl::Interface(i) => {
                    checker.declare_type(&i.name, i.span, out);
                    checker.interfaces.insert(i.name.clone(), i.fields.clone());
                }
                Decl::TypeAlias(t) => checker.declare_type(&t.name, t.span, out),
                Decl::Component(c) => {
                    checker.declare_component(&c.name, c.span, out);
                    if let Some(props) = &c.props_type {
                        checker
                            .component_props
                            .insert(c.name.clone(), props.clone());
                    }
                }
                Decl::Function(f) => checker.declare_function(&f.name, f.span, out),
                _ => {}
            }
        }
        // Pass 2: validasi isi.
        for decl in &module.decls {
            match decl {
                Decl::Component(c) => checker.check_component(c, out),
                Decl::Function(f) => checker.check_function(f, out),
                // Isi window (root UI) ikut divalidasi: pemakaian komponen,
                // panggilan fungsi, elemen tak dikenal.
                Decl::Window(w) => {
                    for prop in &w.props {
                        checker.check_expr(&prop.value, out);
                    }
                }
                Decl::Interface(i) => {
                    let mut seen = HashSet::new();
                    for field in &i.fields {
                        if !seen.insert(field.name.clone()) {
                            out.push(Diagnostic::error(
                                field.span,
                                format!(
                                    "field `{}` duplikat di interface `{}`",
                                    field.name, i.name
                                ),
                            ));
                        }
                        checker.check_type(&field.ty, out);
                    }
                }
                Decl::TypeAlias(t) => checker.check_type(&t.ty, out),
                _ => {}
            }
        }
    }

    fn declare_type(&mut self, name: &str, span: crate::span::Span, out: &mut Vec<Diagnostic>) {
        if !self.type_names.insert(name.to_string()) {
            out.push(Diagnostic::error(
                span,
                format!("tipe `{name}` dideklarasikan dua kali"),
            ));
        }
    }

    fn declare_function(&mut self, name: &str, span: crate::span::Span, out: &mut Vec<Diagnostic>) {
        if !self.function_names.insert(name.to_string()) {
            out.push(Diagnostic::error(
                span,
                format!("fungsi `{name}` dideklarasikan dua kali"),
            ));
        }
    }

    /// Validasi fungsi top-level: tipe return + isi body.
    fn check_function(&self, f: &FunctionDecl, out: &mut Vec<Diagnostic>) {
        for p in &f.params {
            if let Some(t) = &p.ty {
                self.check_type(t, out);
            }
        }
        if let Some(ret) = &f.ret {
            self.check_type(ret, out);
        }
        for stmt in &f.body {
            self.check_stmt(stmt, out);
        }
    }

    fn declare_component(
        &mut self,
        name: &str,
        span: crate::span::Span,
        out: &mut Vec<Diagnostic>,
    ) {
        if !self.component_names.insert(name.to_string()) {
            out.push(Diagnostic::error(
                span,
                format!("komponen `{name}` dideklarasikan dua kali"),
            ));
        }
    }

    fn check_component(&self, c: &ComponentDecl, out: &mut Vec<Diagnostic>) {
        if let Some(props) = &c.props_type {
            if !self.type_names.contains(props) {
                out.push(Diagnostic::error(
                    c.span,
                    format!("tipe props `{props}` tidak dikenal"),
                ));
            } else if !self.interfaces.contains_key(props) {
                out.push(Diagnostic::warning(
                    c.span,
                    format!("tipe props `{props}` bukan interface — validasi atribut dilewati"),
                ));
            }
        }
        for method in &c.methods {
            if let Some(ret) = &method.ret {
                self.check_type(ret, out);
            }
            for stmt in &method.body {
                self.check_stmt(stmt, out);
            }
        }
    }

    /// Validasi pemakaian `<Comp attr={...}/>` terhadap interface props:
    /// atribut tak dikenal (warning), field wajib hilang (error), dan
    /// literal yang tipenya jelas salah (error). Nilai non-literal
    /// (panggilan, variabel, state) dilewati — inferensi penuh = roadmap.
    fn check_component_use(&self, tag: &str, el: &crate::ast::Element, out: &mut Vec<Diagnostic>) {
        let Some(props_type) = self.component_props.get(tag) else {
            return;
        };
        let Some(fields) = self.interfaces.get(props_type) else {
            return;
        };
        for attr in &el.attrs {
            let Some(field) = fields.iter().find(|f| f.name == attr.name) else {
                out.push(Diagnostic::warning(
                    attr.span,
                    format!("atribut `{}` tidak ada di `{props_type}`", attr.name),
                ));
                continue;
            };
            if let Some(v) = &attr.value {
                if let Some(ok) = lit_matches(&field.ty, v) {
                    if !ok {
                        out.push(Diagnostic::error(
                            attr.span,
                            format!("atribut `{}` harus `{}`, bukan `{v}`", attr.name, field.ty),
                        ));
                    }
                }
            }
        }
        for field in fields {
            if !field.optional
                && !el
                    .attrs
                    .iter()
                    .any(|a| a.name == field.name && a.value.is_some())
            {
                out.push(Diagnostic::error(
                    el.span,
                    format!("atribut wajib `{}` hilang di `<{tag}>`", field.name),
                ));
            }
        }
    }

    fn check_stmt(&self, stmt: &Stmt, out: &mut Vec<Diagnostic>) {
        match stmt {
            Stmt::Return(e, _) | Stmt::ExprStmt(e, _) => self.check_expr(e, out),
            Stmt::If {
                cond,
                then_branch,
                else_branch,
                ..
            } => {
                self.check_expr(cond, out);
                for s in then_branch.iter().chain(else_branch) {
                    self.check_stmt(s, out);
                }
            }
            Stmt::For { iter, body, .. } => {
                self.check_expr(iter, out);
                for s in body {
                    self.check_stmt(s, out);
                }
            }
            Stmt::Let { ty, init, .. } => {
                if let Some(t) = ty {
                    self.check_type(t, out);
                }
                if let Some(e) = init {
                    self.check_expr(e, out);
                }
            }
        }
    }

    fn check_expr(&self, e: &Expr, out: &mut Vec<Diagnostic>) {
        match e {
            // Referensi `props` tanpa konteks → abaikan; nama lain yang jelas
            // tidak dikenal akan dilaporkan bila dimulai huruf besar (tipe/komponen).
            Expr::Ident(name, span)
                if name.chars().next().is_some_and(|c| c.is_uppercase())
                    && !self.component_names.contains(name)
                    && !self.type_names.contains(name) =>
            {
                out.push(Diagnostic::error(
                    *span,
                    format!("nama `{name}` tidak dikenal"),
                ));
            }
            Expr::Element(el, _) => {
                let tag = el.tag.clone();
                if !self.component_names.contains(&tag)
                    && !matches!(
                        tag.as_str(),
                        "button" | "text" | "vstack" | "hstack" | "image" | "input"
                    )
                {
                    out.push(Diagnostic::warning(
                        el.span,
                        format!("elemen `<{tag}>` tidak dikenal"),
                    ));
                }
                // Pemakaian komponen ber-props: validasi atribut vs interface.
                self.check_component_use(&tag, el, out);
                for attr in &el.attrs {
                    if let Some(v) = &attr.value {
                        self.check_expr(v, out);
                    }
                }
                for child in &el.children {
                    self.check_expr(child, out);
                }
            }
            Expr::Member { object, .. }
            | Expr::UnOp {
                operand: object, ..
            } => self.check_expr(object, out),
            Expr::Call { callee, args, .. } => {
                // Panggilan fungsi tak dikenal (ident lowercase) — kecuali
                // intrinsik `puts` dan panggilan member (object.method).
                if let Expr::Ident(name, span) = callee.as_ref() {
                    if !self.function_names.contains(name) && !matches!(name.as_str(), "puts") {
                        out.push(Diagnostic::error(
                            *span,
                            format!("fungsi `{name}` tidak dikenal"),
                        ));
                    }
                } else {
                    self.check_expr(callee, out);
                }
                for a in args {
                    self.check_expr(a, out);
                }
            }
            Expr::BinOp { left, right, .. } => {
                self.check_expr(left, out);
                self.check_expr(right, out);
            }
            Expr::Arrow { params, body, .. } => {
                for p in params {
                    if let Some(t) = &p.ty {
                        self.check_type(t, out);
                    }
                }
                self.check_expr(body, out);
            }
            Expr::Array(items, _) => {
                for item in items {
                    self.check_expr(item, out);
                }
            }
            Expr::Object(props, _) => {
                for p in props {
                    self.check_expr(&p.value, out);
                }
            }
            _ => {}
        }
    }

    fn check_type(&self, t: &crate::ty::Type, out: &mut Vec<Diagnostic>) {
        use crate::ty::Type as T;
        match t {
            T::Named(name) if !self.type_names.contains(name) => {
                out.push(Diagnostic::error(
                    crate::span::Span::default(),
                    format!("tipe `{name}` tidak dikenal"),
                ));
            }
            T::Generic { name, args } => {
                if !self.type_names.contains(name) {
                    out.push(Diagnostic::error(
                        crate::span::Span::default(),
                        format!("tipe generik `{name}` tidak dikenal"),
                    ));
                }
                for a in args {
                    self.check_type(a, out);
                }
            }
            T::Union(parts) => {
                for p in parts {
                    self.check_type(p, out);
                }
            }
            T::Array(inner) | T::Fn { ret: inner, .. } => {
                self.check_type(inner, out);
            }
            _ => {}
        }
    }
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

/// Cocokkan nilai literal terhadap tipe field.
/// `Some(true)` = cocok, `Some(false)` = jelas salah, `None` = tak bisa
/// diputuskan (panggilan, variabel, tipe nominal) → dilewati.
fn lit_matches(ty: &crate::ty::Type, e: &Expr) -> Option<bool> {
    use crate::ty::Type as T;
    enum Lit<'a> {
        Str(&'a str),
        Num(f64),
        Bool,
    }
    let lit = match e {
        Expr::String(s, _) | Expr::Template(s, _) => Lit::Str(s),
        Expr::Number(n, _) => Lit::Num(*n),
        Expr::Bool(_, _) => Lit::Bool,
        _ => return None,
    };
    fn atom(ty: &T, lit: &Lit) -> Option<bool> {
        match (ty, lit) {
            (T::String, Lit::Str(_)) | (T::Number, Lit::Num(_)) | (T::Boolean, Lit::Bool) => {
                Some(true)
            }
            (T::StringLit(w), Lit::Str(s)) => Some(s == w),
            (T::NumberLit(w), Lit::Num(n)) => Some(n == w),
            // Literal punya jenis pasti; primitif jenis lain = salah.
            (T::String | T::Number | T::Boolean | T::StringLit(_) | T::NumberLit(_), _) => {
                Some(false)
            }
            (T::Union(parts), _) => {
                let mut unknown = false;
                for p in parts {
                    match atom(p, lit) {
                        Some(true) => return Some(true),
                        None => unknown = true,
                        Some(false) => {}
                    }
                }
                if unknown {
                    None
                } else {
                    Some(false)
                }
            }
            // Nominal/generik/array/fungsi/elemen → tak diputuskan.
            _ => None,
        }
    }
    atom(ty, &lit)
}

#[cfg(test)]
mod tests {
    use crate::parse_source;

    #[test]
    fn duplicate_component_detected() {
        let src = "component A {} component A {}";
        let (_, diagnostics) = parse_source(src);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn unknown_element_warns() {
        let src = "component A { render(): Element { return <nope/>; } }";
        let (_, diagnostics) = parse_source(src);
        assert!(diagnostics.iter().any(|d| d.message.contains("nope")));
    }

    #[test]
    fn props_sesuai_bersih() {
        let src = r#"
            interface Props { label: string; size?: number; }
            component B(props: Props) {
                render(): Element { return <text content={props.label} />; }
            }
            window App { title: "x"; root: <B label="halo" size={20} />; }
        "#;
        let (_, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
    }

    #[test]
    fn props_wajib_hilang_error() {
        let src = r#"
            interface Props { label: string; }
            component B(props: Props) {
                render(): Element { return <text content={props.label} />; }
            }
            window App { title: "x"; root: <B />; }
        "#;
        let (_, diags) = parse_source(src);
        assert!(diags
            .iter()
            .any(|d| d.is_error() && d.message.contains("label")));
    }

    #[test]
    fn props_opsional_boleh_hilang() {
        let src = r#"
            interface Props { label: string; size?: number; }
            component B(props: Props) {
                render(): Element { return <text content={props.label} />; }
            }
            window App { title: "x"; root: <B label="halo" />; }
        "#;
        let (_, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
    }

    #[test]
    fn props_asing_warning() {
        let src = r#"
            interface Props { label: string; }
            component B(props: Props) {
                render(): Element { return <text content={props.label} />; }
            }
            window App { title: "x"; root: <B label="halo" aneh={1} />; }
        "#;
        let (_, diags) = parse_source(src);
        assert!(diags
            .iter()
            .any(|d| !d.is_error() && d.message.contains("aneh")));
    }

    #[test]
    fn props_tipe_salah_error() {
        let src = r#"
            interface Props { label: string; }
            component B(props: Props) {
                render(): Element { return <text content={props.label} />; }
            }
            window App { title: "x"; root: <B label={5} />; }
        "#;
        let (_, diags) = parse_source(src);
        assert!(diags
            .iter()
            .any(|d| d.is_error() && d.message.contains("label")));
    }

    #[test]
    fn props_union_dicek() {
        let src_ok = r#"
            interface Props { kind: "a" | "b"; }
            component B(props: Props) {
                render(): Element { return <text content="x" />; }
            }
            window App { title: "x"; root: <B kind="a" />; }
        "#;
        let (_, diags) = parse_source(src_ok);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let src_bad = r#"
            interface Props { kind: "a" | "b"; }
            component B(props: Props) {
                render(): Element { return <text content="x" />; }
            }
            window App { title: "x"; root: <B kind="c" />; }
        "#;
        let (_, diags) = parse_source(src_bad);
        assert!(diags
            .iter()
            .any(|d| d.is_error() && d.message.contains("kind")));
    }

    #[test]
    fn props_nonliteral_dilewati() {
        let src = r#"
            interface Props { label: string; }
            component B(props: Props) {
                render(): Element { return <text content={props.label} />; }
            }
            function ambil(): string { return "x"; }
            window App { title: "x"; root: <B label={ambil()} />; }
        "#;
        let (_, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
    }

    #[test]
    fn interface_field_duplikat_error() {
        let src = "interface P { label: string; label: number; }";
        let (_, diags) = parse_source(src);
        assert!(diags
            .iter()
            .any(|d| d.is_error() && d.message.contains("duplikat")));
    }
}
