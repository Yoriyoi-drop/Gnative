//! Pembangun scene UI — node tree deklaratif dari binary `.gn` → `Scene`.
//!
//! Binary hasil kompiler tidak boleh memakai tipe Rust di batas ABI, jadi
//! node tree dikumpulkan sebagai [`UiNode`] lewat panggilan C-ABI di
//! `scene_bridge.rs`. File ini murni data + konversi ke `Scene` — tanpa
//! window maupun event loop, sehingga bisa diuji tanpa GPU/display.
//!
//! Posisi dan ukuran akhir dihitung oleh `Scene::layout()` (lihat
//! `gnative-gfx`), bukan di sini.

use gnative_core::{Color, Rect, Size};
use gnative_gfx::scene::{Node, Quad, Scene, StackDirection, StackNode, TextItem};

/// Node UI deklaratif yang diterima runtime dari binary `.gn`.
#[derive(Debug, Clone, PartialEq)]
pub enum UiNode {
    /// `<text content={...} size={n} />`
    Text { content: String, font_size: f32 },
    /// `<button label={...} />`
    Button { label: String },
    /// `<input placeholder={...} />`
    Input { placeholder: String },
    /// `<vstack gap={n}> children </vstack>`
    VStack { gap: f32, children: Vec<UiNode> },
}

/// Warna teks UI (putih).
pub const TEXT_COLOR: Color = Color::new(1.0, 1.0, 1.0, 1.0);
/// Warna latar tombol.
pub const BUTTON_COLOR: Color = Color::new(0.20, 0.45, 0.85, 1.0);
/// Warna latar field input.
pub const INPUT_COLOR: Color = Color::new(0.18, 0.21, 0.28, 1.0);
/// Lebar tetap elemen interaktif (tombol/input).
pub const CONTROL_WIDTH: f32 = 180.0;
/// Tinggi tetap elemen interaktif (tombol/input).
pub const CONTROL_HEIGHT: f32 = 32.0;
/// Ukuran font label elemen interaktif.
pub const CONTROL_FONT: f32 = 12.0;
/// Warna latar window.
pub const BACKGROUND: Color = Color::new(0.13, 0.17, 0.23, 1.0);

impl UiNode {
    /// Konversi ke node `Scene` (posisi akhir diisi `Scene::layout`).
    pub fn to_scene_node(&self) -> Node {
        match self {
            UiNode::Text { content, font_size } => Node::Text(TextItem {
                rect: Rect::ZERO,
                text: content.clone(),
                font_size: *font_size,
                color: TEXT_COLOR,
            }),
            UiNode::Button { label } => control(BUTTON_COLOR, label),
            UiNode::Input { placeholder } => control(INPUT_COLOR, placeholder),
            UiNode::VStack { gap, children } => Node::Stack(StackNode {
                direction: StackDirection::Vertical,
                gap: *gap,
                children: children.iter().map(UiNode::to_scene_node).collect(),
            }),
        }
    }
}

/// Elemen interaktif: latar berwarna + label teks di atasnya.
fn control(background: Color, label: &str) -> Node {
    Node::Group(vec![
        Node::Quad(
            Quad::new(
                Rect::new(0.0, 0.0, CONTROL_WIDTH, CONTROL_HEIGHT),
                background,
            )
            .with_radius(6.0),
        ),
        Node::Text(TextItem {
            rect: Rect::new(6.0, 0.0, 0.0, CONTROL_HEIGHT),
            text: label.to_string(),
            font_size: CONTROL_FONT,
            color: TEXT_COLOR,
        }),
    ])
}

/// Bangun `Scene` siap render dari node tree UI.
///
/// `root = None` menghasilkan scene kosong (window tanpa UI — dipakai
/// `gnative run` pada project yang belum punya `root`).
pub fn build_scene(size: Size, root: Option<&UiNode>) -> Scene {
    let mut scene = Scene {
        size,
        clear: BACKGROUND,
        nodes: root.map(|r| vec![r.to_scene_node()]).unwrap_or_default(),
    };
    scene.layout();
    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vstack_becomes_stack_of_children() {
        let root = UiNode::VStack {
            gap: 16.0,
            children: vec![
                UiNode::Text {
                    content: "halo".into(),
                    font_size: 32.0,
                },
                UiNode::Button {
                    label: "klik".into(),
                },
            ],
        };
        let node = root.to_scene_node();
        let Node::Stack(stack) = node else {
            panic!("vstack harus menjadi Node::Stack");
        };
        assert_eq!(stack.direction, StackDirection::Vertical);
        assert_eq!(stack.gap, 16.0);
        assert_eq!(stack.children.len(), 2);
    }

    #[test]
    fn button_is_quad_plus_label() {
        let node = UiNode::Button {
            label: "klik".into(),
        }
        .to_scene_node();
        let Node::Group(children) = node else {
            panic!("button harus menjadi Node::Group");
        };
        assert_eq!(children.len(), 2);
        assert!(matches!(children[0], Node::Quad(_)));
        let Node::Text(label) = &children[1] else {
            panic!("anak kedua harus teks label");
        };
        assert_eq!(label.text, "klik");
        assert_eq!(label.font_size, CONTROL_FONT);
    }

    #[test]
    fn build_scene_lays_out_root() {
        let root = UiNode::VStack {
            gap: 10.0,
            children: vec![
                UiNode::Text {
                    content: "halo".into(),
                    font_size: 20.0,
                },
                UiNode::Input {
                    placeholder: "ketik".into(),
                },
            ],
        };
        let scene = build_scene(Size::new(800.0, 600.0), Some(&root));
        assert_eq!(scene.nodes.len(), 1);

        // Setelah layout, anak pertama harus berada di area dalam window.
        let Node::Stack(stack) = &scene.nodes[0] else {
            panic!("root harus stack");
        };
        let Node::Text(text) = &stack.children[0] else {
            panic!("anak pertama harus teks");
        };
        assert!(text.rect.y >= 16.0, "teks tidak di dalam padding: {text:?}");
        assert!(text.rect.y + text.rect.height <= 600.0);
        assert!(text.rect.width > 0.0, "lebar teks harus dihitung layout");
    }

    #[test]
    fn build_scene_without_root_is_empty() {
        let scene = build_scene(Size::new(640.0, 480.0), None);
        assert!(scene.nodes.is_empty());
        assert_eq!(scene.size, Size::new(640.0, 480.0));
    }
}