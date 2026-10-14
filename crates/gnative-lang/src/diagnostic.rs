//! Diagnostik terformat untuk file `.gn`.

use std::fmt;

use crate::span::Span;

/// Tingkat keparahan diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Satu pesan diagnostik dengan posisi source.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn error(span: Span, message: String) -> Self {
        Self {
            severity: Severity::Error,
            span,
            message,
        }
    }

    pub fn warning(span: Span, message: String) -> Self {
        Self {
            severity: Severity::Warning,
            span,
            message,
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{level}[{}]: {}", self.span, self.message)
    }
}

/// Render daftar diagnostic ke satu string (untuk output CLI).
pub fn render_all(diagnostics: &[Diagnostic], source_name: &str) -> String {
    if diagnostics.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for d in diagnostics {
        out.push_str(&format!(
            "  --> {source_name}:{}\n  {}: {}\n",
            d.span,
            match d.severity {
                Severity::Error => "ERROR",
                Severity::Warning => "WARN",
            },
            d.message
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::LineColumn;

    #[test]
    fn display_format() {
        let d = Diagnostic::error(
            Span::new(
                0,
                3,
                LineColumn { line: 1, column: 1 },
                LineColumn { line: 1, column: 4 },
            ),
            "gagal".into(),
        );
        assert!(d.to_string().contains("error[1:1]: gagal"));
    }
}
