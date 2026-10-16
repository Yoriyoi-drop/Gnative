//! Scene graph — representasi renderable UI (bukan DOM).

use gnative_core::{Color, Rect, Size, Vec2};

/// Scene: satu frame yang siap dirender backend.
#[derive(Debug, Clone)]
pub struct Scene {
    pub size: Size,
    pub clear: Color,
    pub nodes: Vec<Node>,
}

/// Node scene.
#[derive(Debug, Clone)]
pub enum Node {
    /// Persegi + warna + radius sudut.
    Quad(Quad),
    /// Teks + ukuran font.
    Text(TextItem),
    /// Susunan flex: posisi anak dihitung layout pass.
    Stack(StackNode),
    /// Kumpulan node.
    Group(Vec<Node>),
    /// Pembungkus klik: `id` aksi + isi yang dirender seperti biasa.
    /// Dipakai tombol `onClick` — hit-test mencari varian ini.
    Action { id: usize, inner: Box<Node> },
}

/// Arah susunan `StackNode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackDirection {
    Horizontal,
    Vertical,
}

/// Susunan flex sederhana.
#[derive(Debug, Clone)]
pub struct StackNode {
    pub direction: StackDirection,
    pub gap: f32,
    pub children: Vec<Node>,
}

/// Quad: persegi berwarna dengan radius sudut.
#[derive(Debug, Clone)]
pub struct Quad {
    pub rect: Rect,
    pub color: Color,
    pub radius: f32,
}

impl Quad {
    pub fn new(rect: Rect, color: Color) -> Self {
        Self {
            rect,
            color,
            radius: 0.0,
        }
    }

    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

/// Teks: string + rect + ukuran font + warna.
#[derive(Debug, Clone)]
pub struct TextItem {
    pub rect: Rect,
    pub text: String,
    pub font_size: f32,
    pub color: Color,
}

impl Scene {
    /// Hitung posisi & ukuran semua node secara rekursif.
    ///
    /// Root diperlakukan sebagai tumpukan vertikal berpadding. Dipanggil
    /// sekali sebelum dirender sehingga backend hanya menggambar `rect`
    /// yang sudah final (renderer wgpu tidak menghitung layout).
    pub fn layout(&mut self) {
        let pad = 16.0;
        let avail = (self.size.width - 2.0 * pad).max(1.0);
        layout_nodes(
            &mut self.nodes,
            pad,
            pad,
            avail,
            StackDirection::Vertical,
            10.0,
        );
    }
}

/// Susun `nodes` dalam kotak `(x, y)` berukuran `avail_w`, arah `dir`.
/// Mengembalikan `(lebar_terpakai, tinggi_terpakai)`.
fn layout_nodes(
    nodes: &mut [Node],
    x: f32,
    y: f32,
    avail_w: f32,
    dir: StackDirection,
    gap: f32,
) -> (f32, f32) {
    match dir {
        StackDirection::Vertical => {
            let mut cy = y;
            let mut max_w: f32 = 0.0;
            for n in nodes.iter_mut() {
                let (w, h) = measure(n, avail_w);
                place(n, x, cy, avail_w, dir, gap);
                cy += h + gap;
                max_w = max_w.max(w);
            }
            (max_w, (cy - y - gap).max(0.0))
        }
        StackDirection::Horizontal => {
            let mut cx = x;
            let mut max_h: f32 = 0.0;
            for n in nodes.iter_mut() {
                let (w, h) = measure(n, avail_w);
                place(n, cx, y, avail_w, dir, gap);
                cx += w + gap;
                max_h = max_h.max(h);
            }
            ((cx - x - gap).max(0.0), max_h)
        }
    }
}

/// Ukuran intrinsik node (tanpa memutasi).
fn measure(node: &Node, avail_w: f32) -> (f32, f32) {
    match node {
        Node::Quad(q) => {
            let w = if q.rect.width > 0.0 {
                q.rect.width
            } else {
                200.0
            };
            let h = if q.rect.height > 0.0 {
                q.rect.height
            } else {
                32.0
            };
            (w, h)
        }
        Node::Text(t) => {
            let w =
                (t.text.chars().count() as f32 * t.font_size * 0.55 + 16.0).min(avail_w.max(40.0));
            let h = t.font_size * 1.4 + 8.0;
            (w, h)
        }
        Node::Group(children) => children
            .iter()
            .map(node_extent)
            .fold((0.0, 0.0), |a, b| (a.0.max(b.0), a.1.max(b.1))),
        Node::Action { inner, .. } => node_extent(inner),
        Node::Stack(s) => measure_stack(s, avail_w),
    }
}

/// Ukuran tumpukan tanpa mutasi (panggil `layout_nodes` saat menempatkan).
fn measure_stack(s: &StackNode, avail_w: f32) -> (f32, f32) {
    let mut cursor = 0.0;
    let mut cross: f32 = 0.0; // lebar (vertikal) / tinggi (horizontal) maksimum
    for c in &s.children {
        let (w, h) = measure(c, avail_w);
        match s.direction {
            StackDirection::Vertical => {
                cursor += h + s.gap;
                cross = cross.max(w);
            }
            StackDirection::Horizontal => {
                cursor += w + s.gap;
                cross = cross.max(h);
            }
        }
    }
    match s.direction {
        StackDirection::Vertical => (cross, (cursor - s.gap).max(0.0)),
        StackDirection::Horizontal => ((cursor - s.gap).max(0.0), cross),
    }
}

/// Ekstentsi (batas kanan-bawah) node relatif terhadap origin-nya.
fn node_extent(node: &Node) -> (f32, f32) {
    match node {
        Node::Quad(q) => (q.rect.x + q.rect.width, q.rect.y + q.rect.height),
        Node::Text(t) => (t.rect.x + t.rect.width, t.rect.y + t.rect.height),
        Node::Group(ch) => ch
            .iter()
            .map(node_extent)
            .fold((0.0, 0.0), |a, b| (a.0.max(b.0), a.1.max(b.1))),
        Node::Action { inner, .. } => node_extent(inner),
        Node::Stack(s) => s
            .children
            .iter()
            .map(node_extent)
            .fold((0.0, 0.0), |a, b| (a.0.max(b.0), a.1.max(b.1))),
    }
}

/// Tempatkan node di `(x, y)` (memutasi `rect`).
fn place(node: &mut Node, x: f32, y: f32, avail_w: f32, _dir: StackDirection, _gap: f32) {
    match node {
        Node::Quad(q) => {
            let w = if q.rect.width > 0.0 {
                q.rect.width
            } else {
                200.0
            };
            let h = if q.rect.height > 0.0 {
                q.rect.height
            } else {
                32.0
            };
            q.rect = Rect::new(x, y, w, h);
        }
        Node::Text(t) => {
            let w =
                (t.text.chars().count() as f32 * t.font_size * 0.55 + 16.0).min(avail_w.max(40.0));
            let h = t.font_size * 1.4 + 8.0;
            t.rect = Rect::new(x, y, w, h);
        }
        Node::Group(children) => {
            for c in children.iter_mut() {
                place_relative(c, x, y, avail_w);
            }
        }
        Node::Action { inner, .. } => {
            place_relative(inner, x, y, avail_w);
        }
        Node::Stack(s) => {
            layout_nodes(&mut s.children, x, y, avail_w, s.direction, s.gap);
        }
    }
}

/// Tempatkan anak yang koordinatnya **relatif** terhadap origin grup.
///
/// Node ber-layout (`Stack`) di-layout ulang di origin grup (P2-09: `Stack`
/// di dalam `Group` sebelumnya hanya digeser, jadi tak pernah ter-layout);
/// node lain cukup digeser agar offset relatifnya (mis. label `+6`) tetap utuh.
fn place_relative(node: &mut Node, x: f32, y: f32, avail_w: f32) {
    match node {
        Node::Stack(s) => {
            layout_nodes(&mut s.children, x, y, avail_w, s.direction, s.gap);
        }
        Node::Group(children) => {
            for c in children.iter_mut() {
                place_relative(c, x, y, avail_w);
            }
        }
        Node::Action { inner, .. } => place_relative(inner, x, y, avail_w),
        Node::Quad(_) | Node::Text(_) => offset_node(node, x, y),
    }
}

/// Geser node (dan anak) sebesar `(dx, dy)`.
fn offset_node(node: &mut Node, dx: f32, dy: f32) {
    match node {
        Node::Quad(q) => {
            q.rect = Rect::new(q.rect.x + dx, q.rect.y + dy, q.rect.width, q.rect.height)
        }
        Node::Text(t) => {
            t.rect = Rect::new(t.rect.x + dx, t.rect.y + dy, t.rect.width, t.rect.height)
        }
        Node::Group(children) => {
            for c in children.iter_mut() {
                offset_node(c, dx, dy);
            }
        }
        Node::Action { inner, .. } => {
            offset_node(inner, dx, dy);
        }
        Node::Stack(s) => {
            for c in &mut s.children {
                offset_node(c, dx, dy);
            }
        }
    }
}

impl Scene {
    /// Cari aksi klik pada titik `(x, y)` (koordinat sama seperti layout).
    ///
    /// `actions` = daftar `(id, komponen, method)` dari builder runtime.
    /// Mengembalikan `(komponen, method)` aksi paling dalam yang memuat titik.
    pub fn action_at(
        &self,
        actions: &[(usize, String, String)],
        x: f32,
        y: f32,
    ) -> Option<(String, String)> {
        let pt = Vec2::new(x, y);
        // P2-09: iterasi dari node paling atas (terakhir digambar) ke bawah
        // sehingga aksi yang menimpa (overlap) menang — bukan node bawah.
        let id = self.nodes.iter().rev().find_map(|n| hit_action(n, pt))?;
        actions
            .iter()
            .find(|(i, _, _)| *i == id)
            .map(|(_, c, m)| (c.clone(), m.clone()))
    }
}

/// Aksi paling atas (terakhir digambar) di subtree `node` yang memuat `pt`.
fn hit_action(node: &Node, pt: Vec2) -> Option<usize> {
    match node {
        Node::Action { id, inner } => {
            // Aksi yang lebih dalam (digambar di atas isi) menang lebih dulu;
            // bila tidak ada, aksi ini menang saat bbox-nya memuat titik.
            hit_action(inner, pt).or_else(|| bbox(inner).filter(|r| r.contains(pt)).map(|_| *id))
        }
        // Anak digambar berurutan → anak terakhir paling atas → telusuri mundur.
        Node::Group(children) => children.iter().rev().find_map(|c| hit_action(c, pt)),
        Node::Stack(s) => s.children.iter().rev().find_map(|c| hit_action(c, pt)),
        Node::Quad(_) | Node::Text(_) => None,
    }
}

/// Kotak pembatas gabungan semua daun di bawah `node` (`None` bila kosong).
fn bbox(node: &Node) -> Option<Rect> {
    match node {
        Node::Quad(q) => Some(q.rect),
        Node::Text(t) => Some(t.rect),
        Node::Group(children) => union_all(children.iter().filter_map(bbox)),
        Node::Stack(s) => union_all(s.children.iter().filter_map(bbox)),
        Node::Action { inner, .. } => bbox(inner),
    }
}

/// Gabungan beberapa rect (`None` bila iterator kosong).
fn union_all(rects: impl Iterator<Item = Rect>) -> Option<Rect> {
    let mut out: Option<Rect> = None;
    for r in rects {
        out = Some(match out {
            None => r,
            Some(a) => a.union(&r),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scene uji: tombol Polo (10,10,100x40) + teks biasa.
    fn scene_uji() -> (Scene, Vec<(usize, String, String)>) {
        let button = Node::Group(vec![
            Node::Quad(Quad::new(
                Rect::new(10.0, 10.0, 100.0, 40.0),
                Color::new(0.2, 0.5, 0.85, 1.0),
            )),
            Node::Text(TextItem {
                rect: Rect::new(22.0, 19.0, 0.0, 0.0),
                text: "+".into(),
                font_size: 14.0,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            }),
        ]);
        let scene = Scene {
            size: Size::new(800.0, 600.0),
            clear: Color::new(0.0, 0.0, 0.0, 1.0),
            nodes: vec![
                Node::Action {
                    id: 7,
                    inner: Box::new(button),
                },
                Node::Text(TextItem {
                    rect: Rect::new(200.0, 200.0, 50.0, 20.0),
                    text: "teks".into(),
                    font_size: 14.0,
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                }),
            ],
        };
        let actions = vec![(7usize, "Counter".to_string(), "inc".to_string())];
        (scene, actions)
    }

    #[test]
    fn klik_dalam_tombol_kena() {
        let (scene, actions) = scene_uji();
        assert_eq!(
            scene.action_at(&actions, 50.0, 30.0),
            Some(("Counter".to_string(), "inc".to_string()))
        );
    }

    #[test]
    fn klik_luar_tombol_meleset() {
        let (scene, actions) = scene_uji();
        assert_eq!(scene.action_at(&actions, 500.0, 500.0), None);
        // Di atas teks biasa (bukan aksi) → None.
        assert_eq!(scene.action_at(&actions, 210.0, 210.0), None);
    }

    #[test]
    fn action_tidak_mengubah_layout() {
        // Action meneruskan layout ke isi (posisi anak absolut).
        let (mut scene, _) = scene_uji();
        scene.layout();
        match &scene.nodes[0] {
            Node::Action { inner, .. } => match inner.as_ref() {
                Node::Group(children) => match &children[0] {
                    Node::Quad(q) => assert!(q.rect.x >= 0.0 && q.rect.width > 0.0),
                    other => panic!("anak pertama harus quad, dapat {other:?}"),
                },
                other => panic!("isi harus group, dapat {other:?}"),
            },
            other => panic!("node harus action, dapat {other:?}"),
        }
    }

    /// P2-09: aksi yang menimpa → node paling atas (terakhir) menang.
    #[test]
    fn overlap_aksi_atas_menang() {
        let mk = |id: usize| Node::Action {
            id,
            inner: Box::new(Node::Quad(Quad::new(
                Rect::new(0.0, 0.0, 100.0, 100.0),
                Color::new(0.0, 0.0, 0.0, 1.0),
            ))),
        };
        let scene = Scene {
            size: Size::new(200.0, 200.0),
            clear: Color::new(0.0, 0.0, 0.0, 1.0),
            // id 1 digambar lebih dulu (bawah), id 2 di atas.
            nodes: vec![mk(1), mk(2)],
        };
        let actions = vec![
            (1usize, "A".to_string(), "a".to_string()),
            (2usize, "B".to_string(), "b".to_string()),
        ];
        assert_eq!(
            scene.action_at(&actions, 50.0, 50.0),
            Some(("B".to_string(), "b".to_string())),
            "aksi paling atas harus menang saat overlap"
        );
    }

    /// P2-09: `Stack` di dalam `Group` harus ikut ter-layout.
    #[test]
    fn stack_dalam_group_ter_layout() {
        let stack = Node::Stack(StackNode {
            direction: StackDirection::Vertical,
            gap: 4.0,
            children: vec![
                Node::Text(TextItem {
                    rect: Rect::ZERO,
                    text: "a".into(),
                    font_size: 16.0,
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                }),
                Node::Text(TextItem {
                    rect: Rect::ZERO,
                    text: "b".into(),
                    font_size: 16.0,
                    color: Color::new(1.0, 1.0, 1.0, 1.0),
                }),
            ],
        });
        let mut scene = Scene {
            size: Size::new(400.0, 400.0),
            clear: Color::new(0.0, 0.0, 0.0, 1.0),
            nodes: vec![Node::Group(vec![stack])],
        };
        scene.layout();
        let Node::Group(g) = &scene.nodes[0] else {
            panic!("root harus group");
        };
        let Node::Stack(s) = &g[0] else {
            panic!("anak harus stack");
        };
        let Node::Text(a) = &s.children[0] else {
            panic!("anak stack harus teks");
        };
        let Node::Text(b) = &s.children[1] else {
            panic!("anak stack harus teks");
        };
        assert!(a.rect.width > 0.0, "teks harus ter-layout: {a:?}");
        assert!(
            b.rect.y > a.rect.y,
            "anak kedua harus di bawah anak pertama (a={:?}, b={:?})",
            a.rect,
            b.rect
        );
    }
}
