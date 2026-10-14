//! `Span` — posisi token dalam source (offset + baris/kolom).

use std::fmt;

/// Posisi baris/kolom (1-based) untuk pesan error yang ramah.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineColumn {
    pub line: usize,
    pub column: usize,
}

/// Rentang posisi dalam satu source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Offset byte awal (0-based).
    pub start: usize,
    /// Offset byte akhir (0-based, eksklusif).
    pub end: usize,
    /// Posisi awal untuk ditampilkan.
    pub start_loc: LineColumn,
    /// Posisi akhir untuk ditampilkan.
    pub end_loc: LineColumn,
}

impl Span {
    pub fn new(start: usize, end: usize, start_loc: LineColumn, end_loc: LineColumn) -> Self {
        Self {
            start,
            end,
            start_loc,
            end_loc,
        }
    }

    /// Gabungkan dua span yang berurutan.
    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start,
            end: other.end,
            start_loc: self.start_loc,
            end_loc: other.end_loc,
        }
    }
}

impl Default for Span {
    fn default() -> Self {
        Self {
            start: 0,
            end: 0,
            start_loc: LineColumn { line: 1, column: 1 },
            end_loc: LineColumn { line: 1, column: 1 },
        }
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.start_loc.line, self.start_loc.column)
    }
}
