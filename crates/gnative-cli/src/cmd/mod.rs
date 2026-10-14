//! Dispatch perintah CLI. Satu file = satu perintah.

pub mod build;
pub mod check;
pub mod clean;
pub mod container;
pub mod dev;
pub mod doctor;
pub mod graph;
pub mod install;
pub mod list;
pub mod new;
pub mod publish;
pub mod run;

/// Dispatch argumen ke perintah yang sesuai.
pub fn dispatch(args: Vec<String>) -> i32 {
    let Some(command) = args.first() else {
        print_help();
        return 0;
    };

    let rest = &args[1..];
    match command.as_str() {
        // Alias gaya npm.
        "init" => new::run(rest),
        "start" => run::run(rest),
        "test" => check::run(rest),
        "ls" => list::run(rest),
        "dev" => dev::run(rest),
        // Perintah utama.
        "new" => new::run(rest),
        "build" => build::run(rest),
        "run" => run::run(rest),
        "check" => check::run(rest),
        "clean" => clean::run(rest),
        "graph" => graph::run(rest),
        "doctor" => doctor::run(rest),
        "container" => container::run(rest),
        "install" => install::run(rest),
        "list" => list::run(rest),
        "publish" => publish::run(rest),
        "help" | "--help" | "-h" => {
            print_help();
            0
        }
        other => {
            eprintln!("perintah tidak dikenal: {other}");
            print_help();
            1
        }
    }
}

fn print_help() {
    println!(
        r#"Gnative {version}
GUI framework + bahasa .gn (TypeScript-like).
Kompiler native: .gn → AST → Semantic IR → MIR → Cranelift (dev) / LLVM (release) → binary.

PEMAKAIAN:
  gnative new <nama>        buat project .gn baru
  gnative build             kompilasi src/*.gn → binary native (backend Cranelift)
  gnative build --release   mode release (backend LLVM 18, optimasi agresif)
  gnative build --target T  cross-compile: triple target (mis. aarch64-unknown-linux-gnu)
  gnative run               kompilasi lalu jalankan binary native
  gnative check             parse + type check saja
  gnative clean             bersihkan container/artifacts & cache
  gnative graph             cetak dependency graph
  gnative doctor            cek environment (linker, runtime .gn)
  gnative container ...     subperintah container (list/inspect/verify)

  gnative init <nama>       alias `new` (gaya npm init)
  gnative install           install package dari gnative.toml (gaya npm install)
  gnative list | ls         tampilkan dependencies + status (gaya npm list)
  gnative start             alias `run` (gaya npm start)
  gnative dev               watch: rebuild + restart otomatis tiap perubahan (gaya npm run dev)
  gnative test              parse + check semua file (gaya npm test)
  gnative publish           publish package ke registry (roadmap 1.1)

  gnative help              bantuan ini
"#,
        version = env!("CARGO_PKG_VERSION")
    );
}
