//! Log CLI dengan level verbosity (P2-14).
//!
//! `--verbose`/`-v` menaikkan level sehingga pesan `debug` ikut tercetak.
//! Pesan seragam berbahasa Indonesia; peringatan/error ke stderr.

use std::sync::atomic::{AtomicBool, Ordering};

/// Apakah mode verbose aktif (di-set sekali di `dispatch`).
static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Aktifkan mode verbose.
pub fn set_verbose() {
    VERBOSE.store(true, Ordering::Relaxed);
}

/// Apakah mode verbose aktif.
pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// Info biasa (selalu tercetak) ke stdout.
pub fn info(msg: impl AsRef<str>) {
    println!("[gnative] {}", msg.as_ref());
}

/// Detail — hanya tercetak bila `--verbose`.
pub fn debug(msg: impl AsRef<str>) {
    if is_verbose() {
        println!("[gnative:debug] {}", msg.as_ref());
    }
}

/// Peringatan (stderr).
pub fn warn(msg: impl AsRef<str>) {
    eprintln!("[gnative:peringatan] {}", msg.as_ref());
}

/// Error (stderr).
pub fn error(msg: impl AsRef<str>) {
    eprintln!("[gnative:error] {}", msg.as_ref());
}
