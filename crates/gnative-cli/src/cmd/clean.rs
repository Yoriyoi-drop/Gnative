//! Perintah `clean` — bersihkan container/artifacts & container/cache.

use std::path::Path;

pub fn run(args: &[String]) -> i32 {
    let all = args.iter().any(|a| a == "--all");
    let container = Path::new("container");
    let mut removed = 0;

    // Artifacts build native (object + binary) & cache.
    for sub in ["artifacts", "cache"] {
        let dir = container.join(sub);
        if dir.exists() {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => {
                    println!("menghapus {}", dir.display());
                    removed += 1;
                }
                Err(e) => {
                    eprintln!("gagal menghapus {}: {e}", dir.display());
                    return 1;
                }
            }
        }
    }

    if all {
        // Hapus juga graph & metadata (paket dipertahankan). Error di-propagasi
        // (P3-04: jangan telan kegagalan hapus).
        for sub in ["graph", "metadata"] {
            let dir = container.join(sub);
            if dir.exists() {
                match std::fs::remove_dir_all(&dir) {
                    Ok(()) => {
                        println!("menghapus {}", dir.display());
                        removed += 1;
                    }
                    Err(e) => {
                        eprintln!("gagal menghapus {}: {e}", dir.display());
                        return 1;
                    }
                }
            }
        }
    }

    if removed == 0 {
        println!("container bersih (tidak ada yang dihapus)");
    }
    0
}
