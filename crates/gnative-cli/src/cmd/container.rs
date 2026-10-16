//! Perintah `container` — subperintah inspeksi container.

use std::path::Path;

use gnative_core::safe_join;

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
    let path = match safe_join(&container_dir().join("packages"), package) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("package `{package}` tidak valid: {e}");
            return 1;
        }
    };
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

    // 1. Runtime .gn harus ter-seed (inti container: `@gnative/ui`).
    let runtime = root.join("packages/@gnative/ui");
    let has_gn = std::fs::read_dir(&runtime)
        .map(|it| {
            it.flatten()
                .any(|e| e.path().extension().is_some_and(|x| x == "gn"))
        })
        .unwrap_or(false);
    if has_gn {
        println!("packages/@gnative/ui: ada (runtime .gn)");
    } else {
        println!(
            "packages/@gnative/ui: belum ada / kosong — jalankan `gnative build` untuk seed runtime"
        );
        issues += 1;
    }

    // 2. metadata/container.json (opsional): bila ada harus objek JSON ber-`name`.
    let metadata_file = root.join("metadata/container.json");
    match std::fs::read_to_string(&metadata_file) {
        Ok(text) => {
            let t = text.trim();
            if t.starts_with('{') && t.ends_with('}') && text.contains("\"name\"") {
                println!("metadata/container.json: valid");
            } else {
                println!("metadata/container.json: tidak valid (butuh objek JSON ber-`name`)");
                issues += 1;
            }
        }
        Err(_) => println!("metadata/container.json: belum ada (opsional)"),
    }

    // 3. artifacts/: konsistensi object vs binary hasil build.
    let artifacts = root.join("artifacts");
    let (mut objects, mut binaries) = (0usize, 0usize);
    if let Ok(entries) = std::fs::read_dir(&artifacts) {
        for e in entries.flatten() {
            match e.path().extension().and_then(|x| x.to_str()) {
                Some("o") => objects += 1,
                Some("gn") => {}
                Some(_) if e.path().is_file() => binaries += 1,
                _ => {}
            }
        }
    }
    if objects > 0 || binaries > 0 {
        println!("artifacts/: {objects} object, {binaries} binary");
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
