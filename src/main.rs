//! Entry point Gnative CLI.
//!
//! Aturan (AGENTS.md): `src/` hanya berisi `main.rs` — tidak ada logika di
//! sini. Semua logika ada di `crates/`; file ini hanya mendelegasikan ke
//! `gnative_cli::run()`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(gnative_cli::run(args));
}
