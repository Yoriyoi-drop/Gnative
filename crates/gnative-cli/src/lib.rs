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
}
