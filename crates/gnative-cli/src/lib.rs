//! # gnative-cli
//!
//! Antarmuka baris perintah Gnative. Satu perintah = satu file di `cmd/`.

pub mod cmd;

/// Jalankan CLI; kembalikan exit code.
///
/// `args` = argumen setelah nama program.
pub fn run(args: Vec<String>) -> i32 {
    cmd::dispatch(args)
}
