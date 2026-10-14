//! Dependency graph antar modul `.gn` + deteksi cycle.

use std::collections::{HashMap, HashSet};

/// Node graph: satu file `.gn` (dinamai relatif terhadap `src/`).
#[derive(Debug, Clone)]
pub struct DepGraph {
    nodes: HashMap<String, HashSet<String>>,
}

impl Default for DepGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl DepGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    /// Tambah node + dependensinya.
    pub fn add_node(&mut self, name: &str, deps: &[String]) {
        self.nodes
            .entry(name.to_string())
            .or_default()
            .extend(deps.iter().cloned());
    }

    pub fn contains(&self, name: &str) -> bool {
        self.nodes.contains_key(name)
    }

    /// Dependensi sebuah node (owned copy).
    pub fn deps_of(&self, name: &str) -> Vec<String> {
        self.nodes
            .get(name)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Deteksi cycle via DFS (3 warna). Kembalikan daftar siklus (jika ada).
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        #[derive(Clone, Copy, PartialEq)]
        enum Color {
            White,
            Gray,
            Black,
        }
        let mut colors: HashMap<String, Color> = HashMap::new();
        let mut stack: Vec<String> = Vec::new();
        let mut cycles = Vec::new();

        fn visit(
            node: &str,
            graph: &DepGraph,
            colors: &mut HashMap<String, Color>,
            stack: &mut Vec<String>,
            cycles: &mut Vec<Vec<String>>,
        ) {
            colors.insert(node.to_string(), Color::Gray);
            stack.push(node.to_string());
            let deps: Vec<String> = graph.deps_of(node);
            for dep in deps {
                match colors.get(&dep) {
                    Some(Color::Gray) => {
                        // Cycle ditemukan: potong stack dari dep.
                        if let Some(pos) = stack.iter().position(|n| n == &dep) {
                            cycles.push(stack[pos..].to_vec());
                        }
                    }
                    Some(Color::White) | None => {
                        visit(&dep, graph, colors, stack, cycles);
                    }
                    _ => {}
                }
            }
            stack.pop();
            colors.insert(node.to_string(), Color::Black);
        }

        let keys: Vec<String> = self.nodes.keys().cloned().collect();
        for key in keys {
            if colors.get(&key).copied().unwrap_or(Color::White) == Color::White {
                visit(&key, self, &mut colors, &mut stack, &mut cycles);
            }
        }
        cycles
    }

    /// Urutan topologis (dependensi sebelum dependen). Kembalikan `None`
    /// jika ada cycle.
    pub fn topo_order(&self) -> Option<Vec<String>> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut in_stack = HashSet::new();

        fn visit(
            node: &str,
            graph: &DepGraph,
            visited: &mut HashSet<String>,
            in_stack: &mut HashSet<String>,
            order: &mut Vec<String>,
        ) -> bool {
            if in_stack.contains(node) {
                return false; // cycle
            }
            if visited.contains(node) {
                return true;
            }
            in_stack.insert(node.to_string());
            let deps: Vec<String> = graph.deps_of(node);
            for dep in deps {
                if !visit(&dep, graph, visited, in_stack, order) {
                    return false;
                }
            }
            in_stack.remove(node);
            visited.insert(node.to_string());
            order.push(node.to_string());
            true
        }

        let keys: Vec<String> = self.nodes.keys().cloned().collect();
        for key in keys {
            if !visit(&key, self, &mut visited, &mut in_stack, &mut order) {
                return None;
            }
        }
        Some(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_detected() {
        let mut g = DepGraph::new();
        g.add_node("a.gn", &["b.gn".into()]);
        g.add_node("b.gn", &["a.gn".into()]);
        assert!(!g.detect_cycles().is_empty());
        assert!(g.topo_order().is_none());
    }

    #[test]
    fn topo_ok() {
        let mut g = DepGraph::new();
        g.add_node("a.gn", &[]);
        g.add_node("b.gn", &["a.gn".into()]);
        let order = g.topo_order().unwrap();
        assert_eq!(order, vec!["a.gn".to_string(), "b.gn".to_string()]);
    }
}
