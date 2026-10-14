//! Tipe error Gnative: `GnError` + alias `GnResult`.

use std::fmt;

/// Error seragam seluruh proyek Gnative.
///
/// Kategori error dipisahkan agar setiap lapisan bisa memetakannya
/// dengan jelas (compiler vs renderer vs backend GPU).
#[derive(Debug)]
pub enum GnError {
    /// Error I/O (file tidak ditemukan, tidak bisa dibaca, dsb.).
    Io(std::io::Error),
    /// Error sintaks/parsing pada file `.gn`.
    Parse(String),
    /// Error semantik/tipe pada file `.gn`.
    Type(String),
    /// Error saat memilih/membuat backend rendering.
    BackendUnavailable(String),
    /// Error runtime rendering.
    Render(String),
    /// Error konfigurasi (gnative.toml, dll).
    Config(String),
    /// Error lain yang tidak terklasifikasi.
    Other(String),
}

impl fmt::Display for GnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GnError::Io(e) => write!(f, "I/O error: {e}"),
            GnError::Parse(msg) => write!(f, "parse error: {msg}"),
            GnError::Type(msg) => write!(f, "type error: {msg}"),
            GnError::BackendUnavailable(msg) => write!(f, "backend unavailable: {msg}"),
            GnError::Render(msg) => write!(f, "render error: {msg}"),
            GnError::Config(msg) => write!(f, "config error: {msg}"),
            GnError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for GnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GnError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for GnError {
    fn from(e: std::io::Error) -> Self {
        GnError::Io(e)
    }
}

/// Alias `Result` standar Gnative.
pub type GnResult<T> = Result<T, GnError>;
