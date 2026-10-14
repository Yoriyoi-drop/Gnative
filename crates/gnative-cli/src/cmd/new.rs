//! Perintah `new` — buat project .gn baru.

use std::path::Path;

use gnative_compiler::config::Config;

pub fn run(args: &[String]) -> i32 {
    let Some(name) = args.first() else {
        eprintln!("pemakaian: gnative new <nama-project>");
        return 1;
    };

    let dir = Path::new(name);
    if dir.exists() {
        eprintln!("direktori `{name}` sudah ada");
        return 1;
    }

    if std::fs::create_dir_all(dir.join("src")).is_err() {
        eprintln!("gagal membuat direktori");
        return 1;
    }

    // gnative.toml
    let config = Config::template(name);
    let _ = std::fs::write(dir.join("gnative.toml"), config);

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
    let _ = std::fs::write(dir.join("src/main.gn"), main_gn);

    println!("project `{name}` dibuat:");
    println!("  {name}/gnative.toml");
    println!("  {name}/src/main.gn");
    println!("\njalankan: cd {name} && gnative build");
    println!("         cd {name} && gnative run");
    0
}
