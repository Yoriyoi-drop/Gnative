//! Perintah `new` — buat project .gn baru.

use std::path::Path;

use gnative_compiler::config::Config;
use gnative_core::safe_join;

use crate::log;

pub fn run(args: &[String]) -> i32 {
    let Some(name) = args.first() else {
        log::error("pemakaian: gnative new <nama-project>");
        return 1;
    };

    // `name` dari argv → sanitasi agar project dibuat di cwd, tidak keluar
    // (P0-09: path traversal, mis. `gnative new ../../tmp/evil`).
    let dir = match safe_join(Path::new("."), name) {
        Ok(d) => d,
        Err(e) => {
            log::error(format!("nama project tidak valid: {e}"));
            return 1;
        }
    };
    if dir.exists() {
        log::error(format!("direktori `{name}` sudah ada"));
        return 1;
    }

    // P3-04: propagasi error tulis/buat direktori (jangan telan kegagalan).
    if let Err(e) = std::fs::create_dir_all(dir.join("src")) {
        log::error(format!("gagal membuat direktori {}: {e}", dir.display()));
        return 1;
    }

    // gnative.toml
    let config = Config::template(name);
    if let Err(e) = std::fs::write(dir.join("gnative.toml"), config) {
        log::error(format!("gagal menulis gnative.toml: {e}"));
        return 1;
    }

    // src/main.gn contoh — fungsi/if/for dikompilasi native (milestone 0.8)
    let main_gn = r#"// src/main.gn — contoh project Gnative
use "@gnative/ui";

// Fungsi nyata: params + return, dikompilasi ke MIR → Cranelift.
function status_label(count: number): string {
  if (count > 0) {
    return "aktif";
  }
  return "pasif";
}

function sum(): number {
  let total: number = 0;
  for (n of [1, 2, 3, 4, 5]) {
    total = total + n;
  }
  return total;
}

function title_text(): string {
  return "Halo Gnative 👋";
}

window App {
  title: title_text();
  size: [800, 600];
  root: <vstack gap={16}>
    <text content={title_text()} size={32} />
    <text content={status_label(sum())} size={16} />
    <button label="Klik Saya" />
    <input placeholder="Klik lalu ketik di sini..." />
  </vstack>;
}
"#;
    if let Err(e) = std::fs::write(dir.join("src/main.gn"), main_gn) {
        log::error(format!("gagal menulis src/main.gn: {e}"));
        return 1;
    }

    log::info(format!("project `{name}` dibuat:"));
    println!("  {name}/gnative.toml");
    println!("  {name}/src/main.gn");
    println!("\njalankan: cd {name} && gnative build");
    println!("         cd {name} && gnative run");
    0
}
