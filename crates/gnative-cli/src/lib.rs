//! # gnative-cli
//!
//! Antarmuka baris perintah Gnative. Satu perintah = satu file di `cmd/`.

pub mod cmd;
pub mod log;
pub mod process;

/// Jalankan CLI; kembalikan exit code.
///
/// `args` = argumen setelah nama program.
pub fn run(args: Vec<String>) -> i32 {
    // Flag global `--verbose`/`-v` dikonsumsi di sini (bukan diteruskan ke
    // perintah), lalu sisanya di-dispatch.
    let mut verbose = false;
    let args: Vec<String> = args
        .into_iter()
        .filter(|a| {
            if a == "--verbose" || a == "-v" {
                verbose = true;
                false
            } else {
                true
            }
        })
        .collect();
    if verbose {
        log::set_verbose();
    }
    cmd::dispatch(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_exit_0() {
        assert_eq!(run(vec!["help".into()]), 0);
    }

    #[test]
    fn tanpa_arg_cetak_help_exit_0() {
        assert_eq!(run(vec![]), 0);
    }

    #[test]
    fn perintah_tak_dikenal_exit_1() {
        assert_eq!(run(vec!["zzz-tak-ada".into()]), 1);
    }

    #[test]
    fn new_traversal_ditolak() {
        // P0-09: nama project ber-`..` ditolak (exit 1), tak menulis apa pun.
        assert_eq!(run(vec!["new".into(), "../../evil-gnative-test".into()]), 1);
        assert!(!std::path::Path::new("../../evil-gnative-test").exists());
    }

    #[test]
    fn container_inspect_traversal_ditolak() {
        // P0-09: arg package ber-`..` ditolak (exit 1).
        assert_eq!(
            run(vec![
                "container".into(),
                "inspect".into(),
                "../../etc".into()
            ]),
            1
        );
    }

    #[test]
    fn verbose_tidak_mengganggu_help() {
        // P2-14: flag global `--verbose` tidak boleh mengganggu dispatch.
        assert_eq!(run(vec!["--verbose".into(), "help".into()]), 0);
    }

    #[test]
    fn doctor_selesai_tanpa_panic() {
        // Doctor selalu selesai dengan exit 0/1 (tergantung environment).
        let code = run(vec!["doctor".into()]);
        assert!(code == 0 || code == 1, "exit code doctor: {code}");
    }
}
