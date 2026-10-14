//! Lowering AST → Semantic IR.
//!
//! AST dari `gnative-lang` diubah menjadi `IrModule` (lihat `ir.rs`).
//! Milestone 0.8: `function`, `let`, `if`/`else`, `for`, dan ekspresi
//! (aritmatika, perbandingan, panggilan) di-lower; konstruksi yang belum
//! didukung menghasilkan diagnostic (roadmap milestone berikutnya).
//! Milestone 0.9: komponen dengan `state` — dikompilasi ke instance runtime.

use gnative_core::GnResult;
use gnative_lang::ast::{BinOp, Decl, Expr, MethodDecl, Module, Stmt, UnOp};
use gnative_lang::ty::Type;

use crate::ir::{
    IrBinOp, IrComponent, IrExpr, IrFunc, IrModule, IrNode, IrParam, IrStateField, IrStmt, IrTy,
    IrWindow,
};

/// Hasil lowering: IR + diagnostic (warning untuk hal yang belum didukung).
#[derive(Debug, Default)]
pub struct LowerOutput {
    pub module: IrModule,
    pub diagnostics: Vec<String>,
}

/// Template komponen stateless (ekspansi compile-time).
struct ComponentTemplate {
    render_expr: Option<Expr>,
    has_state: bool,
}

/// Template komponen dengan state (runtime component).
struct StatefulComponentTemplate {
    name: String,
    state_fields: Vec<gnative_lang::ast::StateField>,
    render_method: MethodDecl,
    other_methods: Vec<MethodDecl>,
}

/// Konversi tipe `.gn` → `IrTy` (primitif didukung codegen; lain → F64).
fn gn_ty_to_ir(ty: &Type) -> IrTy {
    match ty {
        Type::Number | Type::NumberLit(_) => IrTy::F64,
        Type::Boolean => IrTy::Bool,
        Type::String | Type::StringLit(_) => IrTy::Str,
        _ => IrTy::from_name(&ty.to_string()).unwrap_or(IrTy::F64),
    }
}

/// Definisi stateful siap pakai: field + method ter-lower, template
/// `render()` mentah untuk ekspansi per instance (dengan props).
struct StatefulDef {
    state_fields: Vec<IrStateField>,
    render_method: MethodDecl,
    methods: Vec<IrFunc>,
}

/// Konteks penomoran instance selama lowering satu modul.
#[derive(Default)]
struct InstanceCtx {
    /// Komponen → jumlah instance yang sudah dibuat.
    counts: std::collections::HashMap<String, usize>,
    /// Tumpukan ekspansi (penjaga siklus `A` → `B` → `A`).
    expanding: Vec<String>,
}

/// Lower satu modul `.gn` (project) menjadi Semantic IR.
pub fn lower(module: &Module) -> GnResult<LowerOutput> {
    let mut out = LowerOutput::default();

    // Pass 1: kumpulkan komponen (pisahkan stateless vs stateful).
    let mut stateless_components: std::collections::HashMap<String, ComponentTemplate> =
        std::collections::HashMap::new();
    let mut stateful_components: Vec<StatefulComponentTemplate> = Vec::new();

    for decl in &module.decls {
        if let Decl::Component(c) = decl {
            let Some(render_method) = c.methods.iter().find(|m| m.name == "render").cloned() else {
                out.diagnostics.push(format!(
                    "komponen `{}` tidak punya `render(): Element`",
                    c.name
                ));
                continue;
            };
            let other_methods: Vec<MethodDecl> = c
                .methods
                .iter()
                .filter(|m| m.name != "render")
                .cloned()
                .collect();

            if c.state.is_empty() {
                // Stateless → compile-time expansion
                stateless_components.insert(
                    c.name.clone(),
                    ComponentTemplate {
                        render_expr: render_method.body.iter().find_map(|s| match s {
                            Stmt::Return(e, _) => Some(e.clone()),
                            _ => None,
                        }),
                        has_state: false,
                    },
                );
            } else {
                // Stateful → definisi runtime (render di-lower per instance)
                stateful_components.push(StatefulComponentTemplate {
                    name: c.name.clone(),
                    state_fields: c.state.clone(),
                    render_method,
                    other_methods,
                });
            }
        }
    }

    // Lower definisi stateful (field + method; gagal → diagnostic, bukan fatal).
    let mut stateful_defs: std::collections::HashMap<String, StatefulDef> =
        std::collections::HashMap::new();
    for tmpl in &stateful_components {
        match lower_stateful_def(tmpl) {
            Ok((name, def)) => {
                out.module.components.push(IrComponent {
                    name: name.clone(),
                    state_fields: def.state_fields.clone(),
                    methods: def.methods.clone(),
                });
                stateful_defs.insert(name, def);
            }
            Err(e) => out
                .diagnostics
                .push(format!("komponen `{}`: {e}", tmpl.name)),
        }
    }

    // Validasi render tiap definisi (coba lower dengan props dummy) agar
    // kesalahan method/onClick tertangkap walau komponen belum dipakai.
    // Instance nyata tetap di-lower per situs (`lower_instance`).
    let def_names: Vec<String> = stateful_defs.keys().cloned().collect();
    for name in &def_names {
        let dummy_el = gnative_lang::ast::Element {
            tag: name.clone(),
            attrs: Vec::new(),
            children: Vec::new(),
            span: gnative_lang::span::Span::default(),
        };
        let mut trial = InstanceCtx::default();
        // Sediakan props dummy bila template membutuhkannya.
        let mut trial_el = dummy_el;
        if let Some(def) = stateful_defs.get(name) {
            if let Some(ret) = def.render_method.body.iter().find_map(|s| match s {
                Stmt::Return(e, _) => Some(e.clone()),
                _ => None,
            }) {
                for p in required_props(&ret) {
                    trial_el.attrs.push(gnative_lang::ast::Attr {
                        name: p,
                        value: Some(Expr::String(String::new(), trial_el.span)),
                        span: trial_el.span,
                    });
                }
            }
        }
        if let Err(e) = lower_instance(&trial_el, &stateful_defs, &mut trial) {
            out.diagnostics.push(format!("komponen `{name}`: {e}"));
        }
    }

    let mut ictx = InstanceCtx::default();

    for decl in &module.decls {
        match decl {
            Decl::Window(w) => {
                match lower_window(w, &stateless_components, &stateful_defs, &mut ictx) {
                    Ok(window) => out.module.window = Some(window),
                    Err(e) => out.diagnostics.push(format!("window `{}`: {e}", w.name)),
                }
            }
            Decl::Function(f) => {
                if f.name == "puts" {
                    continue;
                }
                match lower_function(f) {
                    Ok(func) => out.module.funcs.push(func),
                    Err(e) => {
                        out.diagnostics.push(format!("fungsi `{}`: {e}", f.name));
                    }
                }
            }
            Decl::Component(_) => {
                // Sudah diproses di pass 1
            }
            Decl::Interface(_) | Decl::TypeAlias(_) | Decl::Const(_) => {}
        }
    }

    Ok(out)
}

/// Lower `function name(params): ret { body }`.
fn lower_function(f: &gnative_lang::ast::FunctionDecl) -> Result<IrFunc, String> {
    let mut params = Vec::new();
    for p in &f.params {
        let ty = match &p.ty {
            Some(t) => IrTy::from_name(&t.to_string()).unwrap_or(IrTy::F64),
            None => IrTy::F64, // tanpa anotasi → number (JS-like)
        };
        params.push(IrParam {
            name: p.name.clone(),
            ty,
        });
    }
    let ret = f.ret.as_ref().and_then(|t| IrTy::from_name(&t.to_string()));
    let body = lower_stmts(&f.body)?;
    Ok(IrFunc {
        name: f.name.clone(),
        params,
        ret,
        body,
    })
}

/// Lower definisi komponen stateful: field + method.
/// Render TIDAK di-lower di sini — di-lower per instance pemakaian
/// (dengan props disubstitusi) oleh `lower_instance`.
fn lower_stateful_def(tmpl: &StatefulComponentTemplate) -> Result<(String, StatefulDef), String> {
    // Lower state fields (`ty: Type`, `init: Option<Expr>`)
    let mut state_fields = Vec::new();
    for sf in &tmpl.state_fields {
        let ty = gn_ty_to_ir(&sf.ty);
        let init = match &sf.init {
            Some(e) => lower_expr(e)?,
            None => match ty {
                IrTy::Bool => IrExpr::Bool(false),
                IrTy::Str => IrExpr::Str(String::new()),
                IrTy::F64 => IrExpr::F64(0.0),
            },
        };
        state_fields.push(IrStateField {
            name: sf.name.clone(),
            ty,
            init,
        });
    }

    // Render harus `return <Element/>` (diperiksa per instance; cek bentuk di sini).
    let has_element = tmpl.render_method.body.iter().any(|s| match s {
        Stmt::Return(e, _) => matches!(e, Expr::Element(..)),
        _ => false,
    });
    if !has_element {
        return Err("tidak punya `return <Element/>` di `render()`".into());
    }

    // Method lain (params dipertahankan; `this` implisit).
    let mut methods = Vec::new();
    for m in &tmpl.other_methods {
        methods.push(lower_component_method(m, &state_fields)?);
    }

    let name = tmpl.name.clone();
    Ok((
        name.clone(),
        StatefulDef {
            state_fields,
            render_method: tmpl.render_method.clone(),
            methods,
        },
    ))
}

/// Lower satu situs pemakaian `<Comp attr={...}/>` → `ComponentInstance`.
///
/// Memberi nomor instance, substitusi props ke template `render()`,
/// lalu lower dengan konteks state instance (mendukung nesting +
/// penjaga siklus via `ictx.expanding`).
#[allow(clippy::too_many_arguments)]
fn lower_instance(
    el: &gnative_lang::ast::Element,
    defs: &std::collections::HashMap<String, StatefulDef>,
    ictx: &mut InstanceCtx,
) -> Result<IrNode, String> {
    let name = el.tag.clone();
    let def = defs
        .get(&name)
        .ok_or_else(|| format!("elemen `<{name}>` belum didukung codegen native (roadmap)"))?;
    if ictx.expanding.contains(&name) {
        return Err(format!(
            "siklus komponen `{}` (render memuat dirinya sendiri)",
            ictx.expanding.join(" → ") + " → " + &name
        ));
    }
    if !el.children.is_empty() {
        return Err(format!(
            "children pada `<{name}>` stateful belum didukung (roadmap)"
        ));
    }
    let idx = ictx.counts.get(&name).copied().unwrap_or(0);
    ictx.counts.insert(name.clone(), idx + 1);
    let key = crate::ir::instance_key(&name, idx);

    // Props: lower nilai di konteks induk (tanpa state), lalu substitusi.
    let mut lowered_props = Vec::new();
    let mut prop_asts: std::collections::HashMap<String, Expr> = std::collections::HashMap::new();
    for a in &el.attrs {
        match &a.value {
            Some(v) => {
                let lv = lower_expr(v)?;
                lowered_props.push((a.name.clone(), lv));
                prop_asts.insert(a.name.clone(), v.clone());
            }
            None => {
                return Err(format!("atribut `{}` butuh nilai `={{...}}`", a.name));
            }
        }
    }

    let render_src = def.render_method.body.iter().find_map(|s| match s {
        Stmt::Return(e, _) => Some(e.clone()),
        _ => None,
    });
    let render_src =
        render_src.ok_or_else(|| "tidak punya `return <Element/>` di `render()`".to_string())?;
    let substituted =
        substitute_props(&render_src, &prop_asts).map_err(|m| format!("komponen `{name}`: {m}"))?;
    let Expr::Element(render_el, _) = substituted else {
        return Err(format!("`render()` `{name}` harus `return <Element/>`"));
    };

    let mut method_params: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    method_params.insert(
        def.render_method.name.clone(),
        def.render_method.params.len(),
    );
    for m in &def.methods {
        // Jumlah param method dari definisi ter-lower tidak tersimpan;
        // ambil dari template via panjang params IrFunc.
        method_params.insert(m.name.clone(), m.params.len());
    }

    ictx.expanding.push(name.clone());
    let render = expr_node_with_state(
        &render_el,
        &def.state_fields,
        &key,
        &method_params,
        defs,
        ictx,
    );
    ictx.expanding.pop();
    let render = render.map_err(|e| format!("komponen `{name}`: {e}"))?;

    Ok(IrNode::ComponentInstance {
        component_name: name,
        instance: idx,
        props: lowered_props,
        render: Box::new(render),
    })
}

/// Lower method komponen (method lain selain `render`).
/// Method memiliki akses ke `this.field` untuk state fields.
/// Params dipertahankan (dipakai `Comp__method` di MIR).
fn lower_component_method(
    f: &gnative_lang::ast::MethodDecl,
    state_fields: &[IrStateField],
) -> Result<IrFunc, String> {
    let mut params = Vec::new();
    for p in &f.params {
        let ty = match &p.ty {
            Some(t) => gn_ty_to_ir(t),
            None => IrTy::F64,
        };
        params.push(IrParam {
            name: p.name.clone(),
            ty,
        });
    }
    let ret = f.ret.as_ref().and_then(|t| IrTy::from_name(&t.to_string()));
    let mut locals: std::collections::HashSet<String> =
        f.params.iter().map(|p| p.name.clone()).collect();
    let body = lower_stmts_with_state(&f.body, state_fields, &mut locals)?;
    Ok(IrFunc {
        name: f.name.clone(),
        params,
        ret,
        body,
    })
}

/// Lower statement dengan dukungan akses state field.
fn lower_stmts_with_state(
    stmts: &[Stmt],
    state_fields: &[IrStateField],
    locals: &mut std::collections::HashSet<String>,
) -> Result<Vec<IrStmt>, String> {
    let mut out = Vec::new();
    for s in stmts {
        out.push(lower_stmt_with_state(s, state_fields, locals)?);
    }
    Ok(out)
}

/// Lower statement dengan dukungan akses state field.
fn lower_stmt_with_state(
    s: &Stmt,
    state_fields: &[IrStateField],
    locals: &mut std::collections::HashSet<String>,
) -> Result<IrStmt, String> {
    match s {
        Stmt::Return(expr, _) => {
            if matches!(expr, Expr::Null(_)) {
                Ok(IrStmt::Return(None))
            } else {
                Ok(IrStmt::Return(Some(lower_expr_with_state(
                    expr,
                    state_fields,
                    locals,
                )?)))
            }
        }
        Stmt::Let {
            name,
            ty,
            init,
            span,
        } => {
            let _ = span;
            let value = match init {
                Some(e) => Some(lower_expr_with_state(e, state_fields, locals)?),
                None => None,
            };
            let ty = match ty {
                Some(t) => IrTy::from_name(&t.to_string()),
                None => None,
            };
            locals.insert(name.clone());
            Ok(IrStmt::Let {
                name: name.clone(),
                ty,
                value,
            })
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => Ok(IrStmt::If {
            cond: lower_expr_with_state(cond, state_fields, locals)?,
            then_branch: lower_stmts_with_state(then_branch, state_fields, locals)?,
            else_branch: lower_stmts_with_state(else_branch, state_fields, locals)?,
        }),
        Stmt::For {
            var, iter, body, ..
        } => {
            let items = match iter {
                Expr::Array(items, _) => {
                    let mut out = Vec::new();
                    for it in items {
                        out.push(lower_expr_with_state(it, state_fields, locals)?);
                    }
                    out
                }
                other => {
                    return Err(format!(
                        "`for ({var} of ...)` hanya mendukung array literal, bukan `{other}`"
                    ));
                }
            };
            locals.insert(var.clone());
            Ok(IrStmt::For {
                var: var.clone(),
                items,
                body: lower_stmts_with_state(body, state_fields, locals)?,
            })
        }
        Stmt::ExprStmt(e, _) => Ok(IrStmt::Expr(lower_expr_with_state(
            e,
            state_fields,
            locals,
        )?)),
    }
}

/// Lower ekspresi dengan dukungan akses state field (`this.field` atau `field`).
/// `locals` = variabel lokal/method-param yang membayangi state.
#[allow(clippy::collapsible_if)]
fn lower_expr_with_state(
    e: &Expr,
    state_fields: &[IrStateField],
    locals: &std::collections::HashSet<String>,
) -> Result<IrExpr, String> {
    let is_state = |n: &str| !locals.contains(n) && state_fields.iter().any(|sf| sf.name == n);
    match e {
        Expr::Number(n, _) => Ok(IrExpr::F64(*n)),
        Expr::Bool(b, _) => Ok(IrExpr::Bool(*b)),
        Expr::String(s, _) | Expr::Template(s, _) => Ok(IrExpr::Str(s.clone())),
        Expr::Null(_) => Ok(IrExpr::F64(0.0)),
        Expr::Ident(name, _) => {
            if is_state(name) {
                Ok(IrExpr::StateField(name.clone()))
            } else {
                Ok(IrExpr::Var(name.clone()))
            }
        }
        Expr::Member { object, field, .. } => {
            // `this.field` → state field access
            if let Expr::Ident(obj_name, _) = object.as_ref() {
                if obj_name == "this" {
                    if state_fields.iter().any(|sf| sf.name == *field) {
                        return Ok(IrExpr::StateField(field.clone()));
                    }
                }
            }
            Err(format!("akses member `{object}.{field}` belum didukung"))
        }
        Expr::BinOp {
            left, op, right, ..
        } => {
            let l = lower_expr_with_state(left, state_fields, locals)?;
            let r = lower_expr_with_state(right, state_fields, locals)?;
            if *op == BinOp::Add {
                if let (IrExpr::Str(a), IrExpr::Str(b)) = (&l, &r) {
                    return Ok(IrExpr::Str(format!("{a}{b}")));
                }
                if matches!(l, IrExpr::Str(_)) || matches!(r, IrExpr::Str(_)) {
                    return Err(
                        "concat string hanya untuk literal (variabel/fungsi: roadmap)".into(),
                    );
                }
            }
            let bin = match op {
                BinOp::Add => IrBinOp::Add,
                BinOp::Sub => IrBinOp::Sub,
                BinOp::Mul => IrBinOp::Mul,
                BinOp::Div => IrBinOp::Div,
                BinOp::EqEq => IrBinOp::Eq,
                BinOp::NotEq => IrBinOp::Ne,
                BinOp::Lt => IrBinOp::Lt,
                BinOp::Le => IrBinOp::Le,
                BinOp::Gt => IrBinOp::Gt,
                BinOp::Ge => IrBinOp::Ge,
                BinOp::And => IrBinOp::And,
                BinOp::Or => IrBinOp::Or,
            };
            Ok(IrExpr::Bin(bin, Box::new(l), Box::new(r)))
        }
        Expr::UnOp { op, operand, .. } => match op {
            UnOp::Neg => Ok(IrExpr::Neg(Box::new(lower_expr_with_state(
                operand,
                state_fields,
                locals,
            )?))),
            UnOp::Not => Ok(IrExpr::Not(Box::new(lower_expr_with_state(
                operand,
                state_fields,
                locals,
            )?))),
        },
        Expr::Call { callee, args, .. } => {
            let name = match callee.as_ref() {
                Expr::Ident(name, _) => name.clone(),
                Expr::Member { object, field, .. } => {
                    // `this.method(...)` → method call pada component instance
                    if let Expr::Ident(obj_name, _) = object.as_ref() {
                        if obj_name == "this" {
                            let mut lowered_args = Vec::new();
                            for a in args {
                                lowered_args.push(lower_expr_with_state(a, state_fields, locals)?);
                            }
                            return Ok(IrExpr::MethodCall {
                                receiver: Box::new(IrExpr::Var("this".to_string())),
                                method: field.clone(),
                                args: lowered_args,
                            });
                        }
                    }
                    return Err(format!("panggilan member `{callee}` belum didukung"));
                }
                other => {
                    return Err(format!(
                        "panggilan member `{other}` belum didukung (roadmap)"
                    ));
                }
            };
            let mut lowered = Vec::new();
            for a in args {
                lowered.push(lower_expr_with_state(a, state_fields, locals)?);
            }
            Ok(IrExpr::Call(name, lowered))
        }
        Expr::Assign {
            target, op, value, ..
        } => {
            let name = match target.as_ref() {
                Expr::Ident(name, _) => name.clone(),
                Expr::Member { object, field, .. } => {
                    // `this.field = value` → state field assignment
                    if let Expr::Ident(obj_name, _) = object.as_ref() {
                        if obj_name == "this" {
                            if state_fields.iter().any(|sf| sf.name == *field) {
                                let val = lower_expr_with_state(value, state_fields, locals)?;
                                let val = match op {
                                    gnative_lang::ast::AssignOp::Assign => val,
                                    gnative_lang::ast::AssignOp::Add => IrExpr::Bin(
                                        IrBinOp::Add,
                                        Box::new(IrExpr::StateField(field.clone())),
                                        Box::new(val),
                                    ),
                                    gnative_lang::ast::AssignOp::Sub => IrExpr::Bin(
                                        IrBinOp::Sub,
                                        Box::new(IrExpr::StateField(field.clone())),
                                        Box::new(val),
                                    ),
                                };
                                return Ok(IrExpr::Assign(field.clone(), Box::new(val)));
                            }
                        }
                    }
                    return Err(format!(
                        "target assignment `{target}` harus variabel lokal atau this.field (roadmap)"
                    ));
                }
                other => {
                    return Err(format!(
                        "target assignment `{other}` harus variabel lokal atau this.field (roadmap)"
                    ));
                }
            };
            let val = lower_expr_with_state(value, state_fields, locals)?;
            // Bare `count += 1` pada method = mutasi state bila nama cocok
            // dan tidak dibayangi lokal (baca Ident sudah sama; samakan).
            let is_state_field = is_state(name.as_str());
            let base = if is_state_field {
                IrExpr::StateField(name.clone())
            } else {
                IrExpr::Var(name.clone())
            };
            let val = match op {
                gnative_lang::ast::AssignOp::Assign => val,
                gnative_lang::ast::AssignOp::Add => {
                    IrExpr::Bin(IrBinOp::Add, Box::new(base), Box::new(val))
                }
                gnative_lang::ast::AssignOp::Sub => {
                    IrExpr::Bin(IrBinOp::Sub, Box::new(base), Box::new(val))
                }
            };
            Ok(IrExpr::Assign(name, Box::new(val)))
        }
        other => Err(format!(
            "ekspresi `{other}` belum didukung codegen native (roadmap)"
        )),
    }
}

/// Lower blok statement.
fn lower_stmts(stmts: &[Stmt]) -> Result<Vec<IrStmt>, String> {
    let mut out = Vec::new();
    for s in stmts {
        out.push(lower_stmt(s)?);
    }
    Ok(out)
}

/// Lower satu statement.
fn lower_stmt(s: &Stmt) -> Result<IrStmt, String> {
    match s {
        Stmt::Return(expr, _) => {
            if matches!(expr, Expr::Null(_)) {
                Ok(IrStmt::Return(None))
            } else {
                Ok(IrStmt::Return(Some(lower_expr(expr)?)))
            }
        }
        Stmt::Let {
            name,
            ty,
            init,
            span,
        } => {
            let _ = span;
            let value = match init {
                Some(e) => Some(lower_expr(e)?),
                None => None,
            };
            let ty = match ty {
                Some(t) => IrTy::from_name(&t.to_string()),
                None => None,
            };
            Ok(IrStmt::Let {
                name: name.clone(),
                ty,
                value,
            })
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => Ok(IrStmt::If {
            cond: lower_expr(cond)?,
            then_branch: lower_stmts(then_branch)?,
            else_branch: lower_stmts(else_branch)?,
        }),
        Stmt::For {
            var, iter, body, ..
        } => {
            let items = match iter {
                Expr::Array(items, _) => {
                    let mut out = Vec::new();
                    for it in items {
                        out.push(lower_expr(it)?);
                    }
                    out
                }
                other => {
                    return Err(format!(
                        "`for ({var} of ...)` hanya mendukung array literal, bukan `{other}`"
                    ));
                }
            };
            Ok(IrStmt::For {
                var: var.clone(),
                items,
                body: lower_stmts(body)?,
            })
        }
        Stmt::ExprStmt(e, _) => Ok(IrStmt::Expr(lower_expr(e)?)),
    }
}

/// Lower satu ekspresi.
fn lower_expr(e: &Expr) -> Result<IrExpr, String> {
    match e {
        Expr::Number(n, _) => Ok(IrExpr::F64(*n)),
        Expr::Bool(b, _) => Ok(IrExpr::Bool(*b)),
        Expr::String(s, _) | Expr::Template(s, _) => Ok(IrExpr::Str(s.clone())),
        Expr::Null(_) => Ok(IrExpr::F64(0.0)),
        Expr::Ident(name, _) => Ok(IrExpr::Var(name.clone())),
        Expr::BinOp {
            left, op, right, ..
        } => {
            let l = lower_expr(left)?;
            let r = lower_expr(right)?;
            // Concat string literal: `"a" + "b"` → `"ab"` (folding konstanta).
            if *op == BinOp::Add {
                if let (IrExpr::Str(a), IrExpr::Str(b)) = (&l, &r) {
                    return Ok(IrExpr::Str(format!("{a}{b}")));
                }
                // Concat string non-literal (mis. hasil fungsi) belum didukung:
                // butuh allocator string runtime — roadmap. Error jelas lebih
                // baik daripada di-lower jadi aritmatika pointer.
                if matches!(l, IrExpr::Str(_)) || matches!(r, IrExpr::Str(_)) {
                    return Err(
                        "concat string hanya untuk literal (variabel/fungsi: roadmap)".into(),
                    );
                }
            }
            let bin = match op {
                BinOp::Add => IrBinOp::Add,
                BinOp::Sub => IrBinOp::Sub,
                BinOp::Mul => IrBinOp::Mul,
                BinOp::Div => IrBinOp::Div,
                BinOp::EqEq => IrBinOp::Eq,
                BinOp::NotEq => IrBinOp::Ne,
                BinOp::Lt => IrBinOp::Lt,
                BinOp::Le => IrBinOp::Le,
                BinOp::Gt => IrBinOp::Gt,
                BinOp::Ge => IrBinOp::Ge,
                BinOp::And => IrBinOp::And,
                BinOp::Or => IrBinOp::Or,
            };
            Ok(IrExpr::Bin(bin, Box::new(l), Box::new(r)))
        }
        Expr::UnOp { op, operand, .. } => match op {
            UnOp::Neg => Ok(IrExpr::Neg(Box::new(lower_expr(operand)?))),
            UnOp::Not => Ok(IrExpr::Not(Box::new(lower_expr(operand)?))),
        },
        Expr::Call { callee, args, .. } => {
            let name = match callee.as_ref() {
                Expr::Ident(name, _) => name.clone(),
                other => {
                    return Err(format!(
                        "panggilan member `{other}` belum didukung (roadmap)"
                    ));
                }
            };
            let mut lowered = Vec::new();
            for a in args {
                lowered.push(lower_expr(a)?);
            }
            Ok(IrExpr::Call(name, lowered))
        }
        Expr::Assign {
            target, op, value, ..
        } => {
            let name = match target.as_ref() {
                Expr::Ident(name, _) => name.clone(),
                other => {
                    return Err(format!(
                        "target assignment `{other}` harus variabel lokal (roadmap)"
                    ));
                }
            };
            let val = lower_expr(value)?;
            let val = match op {
                gnative_lang::ast::AssignOp::Assign => val,
                gnative_lang::ast::AssignOp::Add => IrExpr::Bin(
                    IrBinOp::Add,
                    Box::new(IrExpr::Var(name.clone())),
                    Box::new(val),
                ),
                gnative_lang::ast::AssignOp::Sub => IrExpr::Bin(
                    IrBinOp::Sub,
                    Box::new(IrExpr::Var(name.clone())),
                    Box::new(val),
                ),
            };
            Ok(IrExpr::Assign(name, Box::new(val)))
        }
        other => Err(format!(
            "ekspresi `{other}` belum didukung codegen native (roadmap)"
        )),
    }
}

/// Lower `window App { title, size, root }`.
fn lower_window(
    w: &gnative_lang::ast::WindowDecl,
    components: &std::collections::HashMap<String, ComponentTemplate>,
    defs: &std::collections::HashMap<String, StatefulDef>,
    ictx: &mut InstanceCtx,
) -> Result<IrWindow, String> {
    let mut title: Option<IrExpr> = None;
    let mut size = (800.0, 600.0);
    let mut root: Option<IrNode> = None;

    for prop in &w.props {
        match prop.name.as_str() {
            "title" => title = Some(lower_expr(&prop.value)?),
            "size" => size = expr_size(&prop.value)?,
            "root" => root = Some(expr_node(&prop.value, components, defs, ictx)?),
            other => {
                return Err(format!("properti window `{other}` belum didukung"));
            }
        }
    }

    Ok(IrWindow {
        title: title.unwrap_or(IrExpr::Str(String::new())),
        size,
        root: root.unwrap_or(IrNode::VStack {
            gap: 0.0,
            children: Vec::new(),
        }),
    })
}

/// Ekspresi `[w, h]`.
fn expr_size(e: &Expr) -> Result<(f64, f64), String> {
    match e {
        Expr::Array(items, _) if items.len() == 2 => {
            let w = expr_number(&items[0])?;
            let h = expr_number(&items[1])?;
            Ok((w, h))
        }
        other => Err(format!("size harus `[w, h]`, bukan `{other}`")),
    }
}

/// Angka literal.
fn expr_number(e: &Expr) -> Result<f64, String> {
    match e {
        Expr::Number(n, _) => Ok(*n),
        other => Err(format!("nilai harus angka, bukan `{other}`")),
    }
}

/// Ambil atribut elemen sebagai ekspresi string.
fn attr_expr(attrs: &[gnative_lang::ast::Attr], name: &str) -> Result<Option<IrExpr>, String> {
    match attrs.iter().find(|a| a.name == name) {
        None => Ok(None),
        Some(a) => match &a.value {
            Some(v) => Ok(Some(lower_expr(v)?)),
            None => Ok(None),
        },
    }
}

/// Ambil atribut elemen sebagai angka literal.
fn attr_number(attrs: &[gnative_lang::ast::Attr], name: &str) -> Result<Option<f64>, String> {
    match attrs.iter().find(|a| a.name == name) {
        None => Ok(None),
        Some(a) => match &a.value {
            Some(v) => Ok(Some(expr_number(v)?)),
            None => Ok(None),
        },
    }
}

/// Elemen deklaratif `<tag attr={...}>children</tag>` → `IrNode`.
/// Komponen stateless diekspansi compile-time; stateful menjadi
/// `ComponentInstance` (nomor instance + props, render di-lower per situs).
fn expr_node(
    e: &Expr,
    components: &std::collections::HashMap<String, ComponentTemplate>,
    defs: &std::collections::HashMap<String, StatefulDef>,
    ictx: &mut InstanceCtx,
) -> Result<IrNode, String> {
    match e {
        Expr::Element(el, _) => {
            let tag = el.tag.as_str();
            match tag {
                "text" => {
                    let content =
                        attr_expr(&el.attrs, "content")?.unwrap_or(IrExpr::Str(String::new()));
                    let size = attr_number(&el.attrs, "size")?.unwrap_or(14.0);
                    Ok(IrNode::Text { content, size })
                }
                "button" => {
                    let label =
                        attr_expr(&el.attrs, "label")?.unwrap_or(IrExpr::Str(String::new()));
                    if el.attrs.iter().any(|a| a.name == "onClick") {
                        return Err(
                            "`onClick` hanya didukung di dalam `render()` komponen stateful".into(),
                        );
                    }
                    Ok(IrNode::Button {
                        label,
                        on_click: None,
                    })
                }
                "input" => {
                    let placeholder =
                        attr_expr(&el.attrs, "placeholder")?.unwrap_or(IrExpr::Str(String::new()));
                    Ok(IrNode::Input { placeholder })
                }
                "vstack" => {
                    let gap = attr_number(&el.attrs, "gap")?.unwrap_or(0.0);
                    let mut children = Vec::new();
                    for child in &el.children {
                        children.push(expr_node(child, components, defs, ictx)?);
                    }
                    Ok(IrNode::VStack { gap, children })
                }
                // Komponen user: stateless → ekspansi; stateful → instance.
                other => match components.get(other) {
                    Some(template) => expand_component(el, template, components, defs, ictx),
                    None if defs.contains_key(other) => lower_instance(el, defs, ictx),
                    None => Err(format!(
                        "elemen `<{other}>` belum didukung codegen native (roadmap)"
                    )),
                },
            }
        }
        other => Err(format!("root harus elemen deklaratif, bukan `{other}`")),
    }
}

/// Elemen di dalam `render()` stateful → `IrNode` (atribut boleh merujuk
/// state via `lower_expr_with_state`; tag built-in + instance stateful lain).
/// `onClick={this.m}` pada `<button>` menjadi `IrClick` (method harus ada
/// dan tanpa parameter — dipanggil sebagai `Comp__i__m()` saat klik).
/// `key` = kunci instance pemilik (`instance_key`); komponen nested
/// mendapat kuncinya sendiri lewat `lower_instance`.
#[allow(clippy::too_many_arguments)]
fn expr_node_with_state(
    el: &gnative_lang::ast::Element,
    state_fields: &[IrStateField],
    key: &str,
    method_params: &std::collections::HashMap<String, usize>,
    defs: &std::collections::HashMap<String, StatefulDef>,
    ictx: &mut InstanceCtx,
) -> Result<IrNode, String> {
    let locals = std::collections::HashSet::new();
    match el.tag.as_str() {
        "text" => {
            let content = match el.attrs.iter().find(|a| a.name == "content") {
                None => IrExpr::Str(String::new()),
                Some(a) => match &a.value {
                    Some(v) => lower_expr_with_state(v, state_fields, &locals)?,
                    None => IrExpr::Str(String::new()),
                },
            };
            if el.attrs.iter().any(|a| a.name == "onClick") {
                return Err("`onClick` hanya didukung pada `<button>`".into());
            }
            let size = attr_number(&el.attrs, "size")?.unwrap_or(14.0);
            Ok(IrNode::Text { content, size })
        }
        "button" => {
            let label = match el.attrs.iter().find(|a| a.name == "label") {
                None => IrExpr::Str(String::new()),
                Some(a) => match &a.value {
                    Some(v) => lower_expr_with_state(v, state_fields, &locals)?,
                    None => IrExpr::Str(String::new()),
                },
            };
            let on_click = match el.attrs.iter().find(|a| a.name == "onClick") {
                None => None,
                Some(a) => Some(click_action(&a.value, key, method_params)?),
            };
            Ok(IrNode::Button { label, on_click })
        }
        "input" => {
            let placeholder = match el.attrs.iter().find(|a| a.name == "placeholder") {
                None => IrExpr::Str(String::new()),
                Some(a) => match &a.value {
                    Some(v) => lower_expr_with_state(v, state_fields, &locals)?,
                    None => IrExpr::Str(String::new()),
                },
            };
            if el.attrs.iter().any(|a| a.name == "onClick") {
                return Err("`onClick` hanya didukung pada `<button>`".into());
            }
            Ok(IrNode::Input { placeholder })
        }
        "vstack" => {
            let gap = attr_number(&el.attrs, "gap")?.unwrap_or(0.0);
            let mut children = Vec::new();
            for child in &el.children {
                match child {
                    Expr::Element(child_el, _) => {
                        children.push(expr_node_with_state(
                            child_el,
                            state_fields,
                            key,
                            method_params,
                            defs,
                            ictx,
                        )?);
                    }
                    other => {
                        return Err(format!(
                            "child vstack stateful harus elemen, bukan `{other}`"
                        ));
                    }
                }
            }
            Ok(IrNode::VStack { gap, children })
        }
        // Instance stateful nested (`<Panel/>` di dalam render).
        other if defs.contains_key(other) => lower_instance(el, defs, ictx),
        other => Err(format!(
            "elemen `<{other}>` dalam `render()` stateful belum didukung (hanya text/button/input/vstack + komponen stateful)"
        )),
    }
}

/// Validasi `onClick={this.method}` → `IrClick` (`key` = kunci instance).
fn click_action(
    value: &Option<Expr>,
    key: &str,
    method_params: &std::collections::HashMap<String, usize>,
) -> Result<crate::ir::IrClick, String> {
    let Some(v) = value else {
        return Err("`onClick` butuh nilai `onClick={this.method}`".into());
    };
    // Panggilan langsung (`onClick={this.inc()}`) ditolak — harus referensi.
    if matches!(v, Expr::Call { .. }) {
        return Err(
            "`onClick` harus referensi method (`onClick={this.inc}`), bukan panggilan".into(),
        );
    }
    let Expr::Member { object, field, .. } = v else {
        return Err(
            "`onClick` harus `onClick={this.method}` (method komponen tanpa argumen)".into(),
        );
    };
    if !matches!(object.as_ref(), Expr::Ident(n, _) if n == "this") {
        return Err(
            "`onClick` harus `onClick={this.method}` (method komponen tanpa argumen)".into(),
        );
    };
    let method = field.clone();
    let Some(params) = method_params.get(&method) else {
        return Err(format!("method `this.{method}` tidak ada di komponen"));
    };
    if *params != 0 {
        return Err(format!(
            "method `this.{method}` punya parameter — handler klik harus tanpa parameter"
        ));
    }
    Ok(crate::ir::IrClick {
        component: key.to_string(),
        method,
    })
}

/// Ekspansi `<Name attr=.../>` → render body komponen dengan `props.field`
/// disubstitusi nilai atribut (compile-time, tanpa overhead runtime).
fn expand_component(
    el: &gnative_lang::ast::Element,
    template: &ComponentTemplate,
    components: &std::collections::HashMap<String, ComponentTemplate>,
    defs: &std::collections::HashMap<String, StatefulDef>,
    ictx: &mut InstanceCtx,
) -> Result<IrNode, String> {
    if template.has_state {
        return Err(format!(
            "komponen `{}` memakai `state` — pakai `<{} />` tanpa ekspansi (internal)",
            el.tag, el.tag
        ));
    }
    let render = template
        .render_expr
        .as_ref()
        .ok_or_else(|| format!("komponen `{}` tidak punya `render(): Element`", el.tag))?;

    // Map `props.field` → nilai atribut (AST), lalu lower.
    let props: std::collections::HashMap<String, Expr> = el
        .attrs
        .iter()
        .filter_map(|a| a.value.clone().map(|v| (a.name.clone(), v)))
        .collect();
    let substituted =
        substitute_props(render, &props).map_err(|m| format!("komponen `{}`: {m}", el.tag))?;
    expr_node(&substituted, components, defs, ictx)
}

/// Kumpulkan nama `props.*` yang dipakai ekspresi (untuk props dummy validasi).
fn required_props(e: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    collect_props(e, &mut out);
    out.sort();
    out.dedup();
    out
}

/// Rekursi bantu `required_props`.
fn collect_props(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Member { object, field, .. } => {
            if matches!(object.as_ref(), Expr::Ident(n, _) if n == "props") {
                out.push(field.clone());
            } else {
                collect_props(object, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            collect_props(callee, out);
            for a in args {
                collect_props(a, out);
            }
        }
        Expr::BinOp { left, right, .. } => {
            collect_props(left, out);
            collect_props(right, out);
        }
        Expr::UnOp { operand, .. } => collect_props(operand, out),
        Expr::Assign { target, value, .. } => {
            collect_props(target, out);
            collect_props(value, out);
        }
        Expr::Array(items, _) => {
            for i in items {
                collect_props(i, out);
            }
        }
        Expr::Object(props, _) => {
            for p in props {
                collect_props(&p.value, out);
            }
        }
        Expr::Element(el, _) => {
            for a in &el.attrs {
                if let Some(v) = &a.value {
                    collect_props(v, out);
                }
            }
            for c in &el.children {
                collect_props(c, out);
            }
        }
        Expr::Arrow { body, .. } => collect_props(body, out),
        _ => {}
    }
}

/// Ganti `props.field` dengan nilai dari map props (rekursif).
fn substitute_props(
    e: &Expr,
    props: &std::collections::HashMap<String, Expr>,
) -> Result<Expr, String> {
    match e {
        // `props.field` (atau `this.props.field` — belum ada `this` di sini).
        Expr::Member {
            object,
            field,
            span,
        } => {
            if let Expr::Ident(name, _) = object.as_ref() {
                if name == "props" {
                    if let Some(value) = props.get(field) {
                        return Ok(value.clone());
                    }
                    return Err(format!("prop `{field}` tidak diberikan"));
                }
            }
            Ok(Expr::Member {
                object: Box::new(substitute_props(object, props)?),
                field: field.clone(),
                span: *span,
            })
        }
        Expr::Number(..)
        | Expr::String(..)
        | Expr::Template(..)
        | Expr::Bool(..)
        | Expr::Null(..)
        | Expr::Ident(..) => Ok(e.clone()),
        Expr::Call { callee, args, span } => Ok(Expr::Call {
            callee: Box::new(substitute_props(callee, props)?),
            args: args
                .iter()
                .map(|a| substitute_props(a, props))
                .collect::<Result<_, _>>()?,
            span: *span,
        }),
        Expr::BinOp {
            left,
            op,
            right,
            span,
        } => Ok(Expr::BinOp {
            left: Box::new(substitute_props(left, props)?),
            op: *op,
            right: Box::new(substitute_props(right, props)?),
            span: *span,
        }),
        Expr::UnOp { op, operand, span } => Ok(Expr::UnOp {
            op: *op,
            operand: Box::new(substitute_props(operand, props)?),
            span: *span,
        }),
        Expr::Assign {
            target,
            op,
            value,
            span,
        } => Ok(Expr::Assign {
            target: Box::new(substitute_props(target, props)?),
            op: *op,
            value: Box::new(substitute_props(value, props)?),
            span: *span,
        }),
        Expr::Arrow { params, body, span } => Ok(Expr::Arrow {
            params: params.clone(),
            body: Box::new(substitute_props(body, props)?),
            span: *span,
        }),
        Expr::Array(items, span) => Ok(Expr::Array(
            items
                .iter()
                .map(|i| substitute_props(i, props))
                .collect::<Result<_, _>>()?,
            *span,
        )),
        Expr::Object(prop_literals, span) => Ok(Expr::Object(
            prop_literals
                .iter()
                .map(|p| {
                    Ok(gnative_lang::ast::PropLiteral {
                        name: p.name.clone(),
                        value: substitute_props(&p.value, props)?,
                        span: p.span,
                    })
                })
                .collect::<Result<_, String>>()?,
            *span,
        )),
        Expr::Element(el, span) => {
            let mut attrs = Vec::new();
            for a in &el.attrs {
                attrs.push(gnative_lang::ast::Attr {
                    name: a.name.clone(),
                    value: match &a.value {
                        Some(v) => Some(substitute_props(v, props)?),
                        None => None,
                    },
                    span: a.span,
                });
            }
            Ok(Expr::Element(
                gnative_lang::ast::Element {
                    tag: el.tag.clone(),
                    attrs,
                    children: el
                        .children
                        .iter()
                        .map(|c| substitute_props(c, props))
                        .collect::<Result<_, _>>()?,
                    span: el.span,
                },
                *span,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gnative_lang::parse_source;

    #[test]
    fn lower_window_basic() {
        let src = r#"
            window App {
                title: "Halo";
                size: [800, 600];
                root: <vstack gap={8}>
                    <text content="Hi" size={32} />
                </vstack>;
            }
        "#;
        let (module, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = lower(&module).unwrap();
        let w = out.module.window.expect("window ada");
        assert!(matches!(w.title, IrExpr::Str(ref s) if s == "Halo"));
        assert_eq!(w.size, (800.0, 600.0));
        match w.root {
            IrNode::VStack { gap, children } => {
                assert_eq!(gap, 8.0);
                assert_eq!(children.len(), 1);
            }
            other => panic!("root harus vstack, dapat {other:?}"),
        }
        assert!(out.diagnostics.is_empty());
    }

    #[test]
    fn lower_function_with_if_and_for() {
        let src = r#"
            function sum(): number {
                let total: number = 0;
                for (n of [1, 2, 3]) {
                    total = total + n;
                }
                if (total > 0) {
                    return total;
                }
                return 0;
            }
            window App { title: "x"; root: <text content="y"/>; }
        "#;
        let (module, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );
        assert_eq!(out.module.funcs.len(), 1);
        let f = &out.module.funcs[0];
        assert_eq!(f.name, "sum");
        assert_eq!(f.ret, Some(IrTy::F64));
        // let + for + if + return = 4 stmt (return di dalam then_branch if)
        assert_eq!(f.body.len(), 4, "body: {:?}", f.body);
    }

    #[test]
    fn component_expands_to_node() {
        // `<Badge label="x"/>` → render body komponen dengan props disubstitusi.
        let src = r#"
            component Badge {
                render(): Element {
                    return <text content={props.label} size={props.size} />;
                }
            }
            window App {
                title: "x";
                root: <vstack>
                    <Badge label="halo" size={20} />
                </vstack>;
            }
        "#;
        let (module, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );
        let w = out.module.window.expect("window ada");
        match w.root {
            IrNode::VStack { children, .. } => {
                assert_eq!(children.len(), 1);
                match &children[0] {
                    IrNode::Text { content, size } => {
                        assert!(matches!(content, IrExpr::Str(ref s) if s == "halo"));
                        assert_eq!(*size, 20.0);
                    }
                    other => panic!("child harus text, dapat {other:?}"),
                }
            }
            other => panic!("root harus vstack, dapat {other:?}"),
        }
    }

    #[test]
    fn component_without_render_errors() {
        let src = r#"
            component Foo {}
            window App { title: "x"; root: <Foo/>; }
        "#;
        let (module, _) = parse_source(src);
        let out = lower(&module).unwrap();
        assert!(out.diagnostics.iter().any(|d| d.contains("render")));
    }

    #[test]
    fn component_missing_prop_errors() {
        let src = r#"
            component Badge {
                render(): Element {
                    return <text content={props.label} />;
                }
            }
            window App { title: "x"; root: <Badge/>; }
        "#;
        let (module, _) = parse_source(src);
        let out = lower(&module).unwrap();
        assert!(out.diagnostics.iter().any(|d| d.contains("label")));
    }

    #[test]
    fn stateful_component_lowers_to_ir() {
        let src = r#"
            component Counter {
                state { count: number = 0; }
                render(): Element {
                    return <text content="x" size={14} />;
                }
                inc(): void { count += 1; }
            }
            window App { title: "x"; root: <Counter/>; }
        "#;
        let (module, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );
        assert_eq!(out.module.components.len(), 1);
        let c = &out.module.components[0];
        assert_eq!(c.name, "Counter");
        assert_eq!(c.state_fields.len(), 1);
        assert_eq!(c.state_fields[0].name, "count");
        assert_eq!(c.methods.len(), 1);
        assert_eq!(c.methods[0].name, "inc");
        // Pemakaian menjadi instance (bukan ekspansi).
        let w = out.module.window.expect("window ada");
        assert!(matches!(
            w.root,
            IrNode::ComponentInstance {
                ref component_name,
                ..
            } if component_name == "Counter"
        ));
    }

    #[test]
    fn stateful_render_state_ref() {
        // `render()` boleh merujuk state (`this.count`).
        let src = r#"
            component Label {
                state { pesan: string = "awal"; }
                render(): Element {
                    return <text content={this.pesan} size={14} />;
                }
            }
            window App { title: "x"; root: <Label/>; }
        "#;
        let (module, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );
        let w = out.module.window.expect("window ada");
        match &w.root {
            IrNode::ComponentInstance { render, .. } => match render.as_ref() {
                IrNode::Text { content, .. } => {
                    assert!(matches!(content, IrExpr::StateField(n) if n == "pesan"));
                }
                other => panic!("render harus text, dapat {other:?}"),
            },
            other => panic!("root harus instance, dapat {other:?}"),
        }
    }

    #[test]
    fn stateful_props_disubstitusi() {
        // Props stateful disubstitusi per instance.
        let src = r#"
            component Badge {
                state { n: number = 0; }
                render(): Element {
                    return <text content={props.label} />;
                }
            }
            window App { title: "x"; root: <Badge label="halo" />; }
        "#;
        let (module, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );
        let w = out.module.window.expect("window ada");
        match &w.root {
            IrNode::ComponentInstance { render, props, .. } => {
                assert_eq!(props.len(), 1);
                match render.as_ref() {
                    IrNode::Text { content, .. } => {
                        assert!(matches!(content, IrExpr::Str(s) if s == "halo"));
                    }
                    other => panic!("render harus text, dapat {other:?}"),
                }
            }
            other => panic!("root harus instance, dapat {other:?}"),
        }
    }

    #[test]
    fn stateful_prop_hilang_error() {
        // Prop wajib yang tak diberikan → diagnostic jelas.
        let src = r#"
            component Badge {
                state { n: number = 0; }
                render(): Element {
                    return <text content={props.label} />;
                }
            }
            window App { title: "x"; root: <Badge />; }
        "#;
        let (module, _) = parse_source(src);
        let out = lower(&module).unwrap();
        assert!(out.diagnostics.iter().any(|d| d.contains("label")));
    }

    #[test]
    fn multi_instance_nomor_urut() {
        // Dua `<Counter/>` → instance 0 dan 1 (state terpisah).
        let src = r#"
            component Counter {
                state { n: number = 0; }
                render(): Element {
                    return <text content="x" />;
                }
            }
            window App { title: "x"; root: <vstack><Counter/><Counter/></vstack>; }
        "#;
        let (module, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );
        match &out.module.window.expect("window ada").root {
            IrNode::VStack { children, .. } => {
                assert_eq!(children.len(), 2);
                match (&children[0], &children[1]) {
                    (
                        IrNode::ComponentInstance { instance: a, .. },
                        IrNode::ComponentInstance { instance: b, .. },
                    ) => assert_eq!((*a, *b), (0, 1)),
                    other => panic!("harus dua instance, dapat {other:?}"),
                }
            }
            other => panic!("root harus vstack, dapat {other:?}"),
        }
    }

    #[test]
    fn siklus_nested_ditolak() {
        // `A` merender `<B/>`, `B` merender `<A/>` → diagnostic siklus.
        let src = r#"
            component A {
                state { n: number = 0; }
                render(): Element { return <vstack><B/></vstack>; }
            }
            component B {
                state { m: number = 0; }
                render(): Element { return <vstack><A/></vstack>; }
            }
            window App { title: "x"; root: <A/>; }
        "#;
        let (module, _) = parse_source(src);
        let out = lower(&module).unwrap();
        assert!(out.diagnostics.iter().any(|d| d.contains("siklus")));
    }

    #[test]
    fn onclick_this_method_ok() {
        let src = r#"
            component Counter {
                state { count: number = 0; }
                render(): Element {
                    return <vstack gap={8}>
                        <button label="+" onClick={this.inc} />
                    </vstack>;
                }
                inc(): void { count += 1; }
            }
            window App { title: "x"; root: <Counter/>; }
        "#;
        let (module, diags) = parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );
        let w = out.module.window.expect("window ada");
        assert_eq!(out.module.components[0].name, "Counter");
        match &w.root {
            IrNode::ComponentInstance { render, .. } => match render.as_ref() {
                IrNode::VStack { children, .. } => match &children[0] {
                    IrNode::Button {
                        on_click: Some(click),
                        ..
                    } => {
                        assert_eq!(click.component, crate::ir::instance_key("Counter", 0));
                        assert_eq!(click.method, "inc");
                    }
                    other => panic!("harus button klik, dapat {other:?}"),
                },
                other => panic!("render harus vstack, dapat {other:?}"),
            },
            other => panic!("root harus instance, dapat {other:?}"),
        }
    }

    #[test]
    fn onclick_bukan_this_ditolak() {
        let src = r#"
            component C {
                state { n: number = 0; }
                render(): Element {
                    return <button label="x" onClick={lain} />;
                }
                inc(): void { n += 1; }
            }
            window App { title: "x"; root: <text content="y"/>; }
        "#;
        let (module, _) = parse_source(src);
        let out = lower(&module).unwrap();
        assert!(out.diagnostics.iter().any(|d| d.contains("onClick")));
    }

    #[test]
    fn onclick_panggilan_ditolak() {
        let src = r#"
            component C {
                state { n: number = 0; }
                render(): Element {
                    return <button label="x" onClick={this.inc()} />;
                }
                inc(): void { n += 1; }
            }
            window App { title: "x"; root: <text content="y"/>; }
        "#;
        let (module, _) = parse_source(src);
        let out = lower(&module).unwrap();
        assert!(out.diagnostics.iter().any(|d| d.contains("referensi")));
    }

    #[test]
    fn onclick_method_tak_ada_ditolak() {
        let src = r#"
            component C {
                state { n: number = 0; }
                render(): Element {
                    return <button label="x" onClick={this.hilang} />;
                }
            }
            window App { title: "x"; root: <text content="y"/>; }
        "#;
        let (module, _) = parse_source(src);
        let out = lower(&module).unwrap();
        assert!(out.diagnostics.iter().any(|d| d.contains("tidak ada")));
    }

    #[test]
    fn onclick_method_berparam_ditolak() {
        let src = r#"
            component C {
                state { n: number = 0; }
                render(): Element {
                    return <button label="x" onClick={this.tambah} />;
                }
                tambah(x: number): void { n += x; }
            }
            window App { title: "x"; root: <text content="y"/>; }
        "#;
        let (module, _) = parse_source(src);
        let out = lower(&module).unwrap();
        assert!(out
            .diagnostics
            .iter()
            .any(|d| d.contains("tanpa parameter")));
    }

    #[test]
    fn onclick_luar_stateful_ditolak() {
        let src = r#"
            component Badge {
                render(): Element {
                    return <button label="x" onClick={this.inc} />;
                }
            }
            window App { title: "x"; root: <Badge/>; }
        "#;
        let (module, _) = parse_source(src);
        let out = lower(&module).unwrap();
        assert!(out.diagnostics.iter().any(|d| d.contains("onClick")));
    }
}
