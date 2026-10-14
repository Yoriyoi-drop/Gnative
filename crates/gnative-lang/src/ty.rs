//! Representasi tipe bahasa `.gn` (structural, ala TypeScript).

use std::fmt;

/// Tipe bahasa `.gn`.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    String,
    Number,
    Boolean,
    Void,
    Null,
    /// Literal string, mis. `"primary"`.
    StringLit(String),
    /// Literal number, mis. `12`.
    NumberLit(f64),
    /// Union, mis. `"primary" | "secondary"`.
    Union(Vec<Type>),
    /// Array, mis. `string[]`.
    Array(Box<Type>),
    /// Fungsi, mis. `() => void`.
    Fn {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    /// Referensi nama tipe/interface.
    Named(String),
    /// Tipe generik, mis. `List<string>`.
    Generic {
        name: String,
        args: Vec<Type>,
    },
    /// Elemen deklaratif `<Button .../>` — hasil `render()`.
    Element,
    /// Tipe yang belum diketahui (error recovery).
    Unknown,
}

impl Type {
    /// Cek apakah tipe ini merupakan bagian dari union literal.
    pub fn is_literal(&self) -> bool {
        matches!(self, Type::StringLit(_) | Type::NumberLit(_))
    }

    /// Representasi singkat untuk codegen.
    pub fn rust_name(&self) -> String {
        match self {
            Type::String => "String".into(),
            Type::Number => "f32".into(),
            Type::Boolean => "bool".into(),
            Type::Void => "()".into(),
            Type::Null => "()".into(),
            Type::StringLit(s) => format!("&'static str /* \"{s}\" */"),
            Type::NumberLit(n) => format!("f32 /* {n} */"),
            Type::Union(_) => "String /* union */".into(),
            Type::Array(inner) => format!("Vec<{}>", inner.rust_name()),
            // Arc agar callback bisa di-klon ke handler scene (event pipeline).
            Type::Fn { params, ret } => format!(
                "Arc<dyn Fn({}) -> {}>",
                params
                    .iter()
                    .map(Type::rust_name)
                    .collect::<Vec<_>>()
                    .join(", "),
                ret.rust_name()
            ),
            Type::Named(n) => n.clone(),
            Type::Generic { name, args } => {
                let args = args
                    .iter()
                    .map(Type::rust_name)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}<{args}>")
            }
            Type::Element => "gnative_ui::Element".into(),
            Type::Unknown => "()".into(),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::String => write!(f, "string"),
            Type::Number => write!(f, "number"),
            Type::Boolean => write!(f, "boolean"),
            Type::Void => write!(f, "void"),
            Type::Null => write!(f, "null"),
            Type::StringLit(s) => write!(f, "\"{s}\""),
            Type::NumberLit(n) => write!(f, "{n}"),
            Type::Union(parts) => {
                let parts: Vec<String> = parts.iter().map(|t| t.to_string()).collect();
                write!(f, "{}", parts.join(" | "))
            }
            Type::Array(inner) => write!(f, "{}[]", inner),
            Type::Fn { params, ret } => {
                let params: Vec<String> = params.iter().map(|t| t.to_string()).collect();
                write!(f, "({}) => {}", params.join(", "), ret)
            }
            Type::Named(n) => write!(f, "{n}"),
            Type::Generic { name, args } => {
                let args: Vec<String> = args.iter().map(|t| t.to_string()).collect();
                write!(f, "{name}<{}>", args.join(", "))
            }
            Type::Element => write!(f, "Element"),
            Type::Unknown => write!(f, "unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_display() {
        let t = Type::Union(vec![
            Type::StringLit("primary".into()),
            Type::StringLit("secondary".into()),
        ]);
        assert_eq!(t.to_string(), "\"primary\" | \"secondary\"");
    }
}
