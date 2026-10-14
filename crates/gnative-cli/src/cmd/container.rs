//! Perintah `container` — subperintah inspeksi container.

use std::path::Path;

pub fn run(args: &[String]) -> i32 {
    let Some(sub) = args.first() else {
        print_usage();
        return 1;
    };
    match sub.as_str() {
        "list" => list(),
        "inspect" => inspect(args.get(1).map(String::as_str)),
        "verify" => verify(),
        _ => {
            eprintln!("subperintah container tidak dikenal: {sub}");
            print_usage();
            1
        }
    }
}

fn print_usage() {
    eprintln!("pemakaian: gnative container <list|inspect|verify>");
}

fn container_dir() -> &'static Path {
    Path::new("container")
}

fn list() -> i32 {
    let root = container_dir();
    println!("isi container:");
    for sub in ["packages", "artifacts", "cache", "graph", "metadata"] {
        let dir = root.join(sub);
        let exists = if dir.exists() { "ada" } else { "—" };
        println!("  {sub:<10} {exists}");
    }
    0
}

fn inspect(package: Option<&str>) -> i32 {
    let Some(package) = package else {
        eprintln!("pemakaian: gnative container inspect <@scope/name>");
        return 1;
    };
    let path = container_dir().join("packages").join(package);
    if !path.exists() {
        eprintln!("package `{package}` tidak ditemukan di {}", path.display());
        return 1;
    }
    println!("package: {package}");
    println!("lokasi:  {}", path.display());
    match std::fs::read_dir(&path) {
        Ok(entries) => {
            for entry in entries.flatten() {
                println!("  {}", entry.file_name().to_string_lossy());
            }
        }
        Err(e) => {
            eprintln!("gagal membaca: {e}");
            return 1;
        }
    }
    0
}

fn verify() -> i32 {
    let root = container_dir();
    let mut issues = 0;

    // Pastikan direktori penting konsisten dengan metadata (jika ada).
    let metadata_file = root.join("metadata/container.json");
    if metadata_file.exists() {
        if let Ok(text) = std::fs::read_to_string(&metadata_file) {
            if !text.contains("gnative") {
                println!("metadata/container.json tampak tidak valid");
                issues += 1;
            }
        }
    } else {
        println!("metadata/container.json belum ada (normal untuk project baru)");
    }

    // Package runtime harus tersedia (di-seed compiler saat build).
    let runtime = root.join("packages/@gnative/ui");
    if runtime.exists() {
        println!("packages/@gnative/ui: ada (runtime .gn)");
    } else {
        println!("packages/@gnative/ui: belum ada — jalankan `gnative build` untuk seed runtime");
    }

    let artifacts = root.join("artifacts");
    if artifacts.exists() {
        println!("artifacts/: ada (object + binary native)");
    } else {
        println!("artifacts/: belum ada — jalankan `gnative build`");
    }

    if issues == 0 {
        println!("container OK");
        0
    } else {
        println!("{issues} masalah ditemukan");
        1
    }
}
