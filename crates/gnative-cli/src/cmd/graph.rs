//! Perintah `graph` — cetak dependency graph project.

use std::path::Path;

use gnative_compiler::graph::DepGraph;
use gnative_lang::parse_source;

pub fn run(_args: &[String]) -> i32 {
    let src_dir = Path::new("src");
    let mut graph = DepGraph::new();
    let mut any = false;

    if let Ok(entries) = std::fs::read_dir(src_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || !path.extension().map(|e| e == "gn").unwrap_or(false) {
                continue;
            }
            any = true;
            let rel = path
                .strip_prefix(src_dir)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            if let Ok(source) = std::fs::read_to_string(&path) {
                let (module, _) = parse_source(&source);
                let imports: Vec<String> = module.imports.iter().map(|i| i.path.clone()).collect();
                graph.add_node(&rel, &imports);
            }
        }
    }

    if !any {
        eprintln!("tidak ada file .gn di src/");
        return 1;
    }

    println!("dependency graph:");
    let keys: Vec<String> = graph.topo_order().unwrap_or_else(|| {
        eprintln!("(ada cycle — urutan topologis gagal)");
        Vec::new()
    });
    for key in keys {
        let deps: Vec<String> = graph.deps_of(&key);
        if deps.is_empty() {
            println!("  {key}");
        } else {
            println!("  {key} → {}", deps.join(", "));
        }
    }
    0
}
