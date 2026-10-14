//! Perintah `publish` — STUB roadmap (analog `npm publish`).
//!
//! Registry package Gnative belum ada (roadmap 1.1). Perintah ini
//! mengembalikan `GnError::BackendUnavailable`-style pesan yang jelas.

pub fn run(_args: &[String]) -> i32 {
    eprintln!(
        "publish belum tersedia — registry package Gnative roadmap 1.1.\n\
         Saat ini distribusi lewat `container/packages` (lokal)."
    );
    1
}
