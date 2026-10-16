//! Perintah `graph` — cetak dependency graph project.

use std::path::Path;

use gnative_compiler::graph::DepGraph;
use gnative_lang::parse_source;

pub fn run(_args: &[String]) -> i32 {
    let src_dir = Path::new("src");
    let mut graph = DepGraph::new();
    let mut any = false;

    // Rekursif: sertakan file .gn di subdirektori (audit P1-14).
    fn walk(dir: &Path, src_dir: &Path, graph: &mut DepGraph, any: &mut bool) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, src_dir, graph, any);
            } else if path.extension().map(|e| e == "gn").unwrap_or(false) {
                *any = true;
                let rel = path
                    .strip_prefix(src_dir)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| path.to_string_lossy().into_owned());
                if let Ok(source) = std::fs::read_to_string(&path) {
                    let (module, _) = parse_source(&source);
                    let imports: Vec<String> =
                        module.imports.iter().map(|i| i.path.clone()).collect();
                    graph.add_node(&rel, &imports);
                }
            }
        }
    }
    walk(src_dir, src_dir, &mut graph, &mut any);

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

    // Siklus → exit non-zero (audit P1-14: bukan exit 0).
    let cycles = graph.detect_cycles();
    if !cycles.is_empty() {
        for c in &cycles {
            eprintln!("siklus: {}", c.join(" → "));
        }
        return 1;
    }
    0
}
