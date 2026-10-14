//! Intrinsik C-ABI runtime — dipanggil binary hasil kompiler `.gn`.
//!
//! Binary yang dihasilkan kompiler (Cranelift → object → linker) memanggil
//! fungsi-fungsi `gn_*` ini sebagai entry point runtime:
//!
//! ```text
//! main (dari .gn)
//!   ├─ gn_window_create(title_ptr, title_len, w, h) → handle
//!   └─ gn_window_run(handle)
//! ```
//!
//! Karena binary `.gn` murni (tanpa runtime Rust), antarmuka dibuat
//! `extern "C"` dengan tipe primitif — tidak ada Rust type di batas ABI.

use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

use gnative_core::{Color, Rect, Size};
use gnative_gfx::scene::{Node, Quad, Scene, StackDirection, StackNode, TextItem};

use crate::event_loop::run_app;

/// Handle window (sederhana: index, karena satu window per aplikasi).
pub type WindowHandle = i32;

const INVALID_HANDLE: WindowHandle = -1;
const HANDLE_MAIN: WindowHandle = 0;

/// Buat window native. Dipanggil `main` hasil kompiler `.gn`.
///
/// # Safety
/// `title_ptr` harus pointer valid ke C string (`title_len` byte).
///
/// `title_len` dipertahankan di ABI (dikirim codegen) tapi tidak dipakai
/// karena string selalu null-terminated.
#[no_mangle]
pub unsafe extern "C" fn gn_window_create(
    title_ptr: *const c_char,
    _title_len: usize,
    width: u32,
    height: u32,
) -> WindowHandle {
    if title_ptr.is_null() {
        return INVALID_HANDLE;
    }
    // String dari MIR dikirim sebagai C string (null-terminated) oleh codegen.
    let title = unsafe { CStr::from_ptr(title_ptr) }
        .to_string_lossy()
        .into_owned();
    // Untuk milestone 0.7: window dibuka saat `gn_window_run` (event loop
    // butuh kontrol utama). Simpan konfigurasi global.
    *pending_state().lock().expect("lock pending") = Some((title, (width, height)));
    HANDLE_MAIN
}

/// Jalankan event loop aplikasi (blocking) sampai window ditutup.
///
/// Kept untuk kompatibilitas: merender `default_scene` (placeholder) bila
/// binary `.gn` belum membangun `Scene` via builder intrinsik.
#[no_mangle]
pub extern "C" fn gn_window_run(_handle: WindowHandle) -> i32 {
    let pending = pending_state().lock().expect("lock pending").take();
    let (title, (width, height)) = pending.unwrap_or_else(|| ("Gnative".to_string(), (800, 600)));
    match run_app(&title, width.max(1), height.max(1), None) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[gnative] runtime error: {e}");
            1
        }
    }
}

/// Jalankan aplikasi dengan `Scene` yang dibangun compiler via builder.
///
/// `scene` adalah handle dari `gn_scene_create` (selalu 1 — satu window).
/// Jalur statis (tanpa rebuild klik); kode baru memakai `gn_window_run_dynamic`.
#[no_mangle]
pub extern "C" fn gn_window_run_scene(_scene: WindowHandle) -> i32 {
    let pending = pending_state().lock().expect("lock pending").take();
    let (title, (width, height)) = pending.unwrap_or_else(|| ("Gnative".to_string(), (800, 600)));

    // Ambil scene yang sudah dirakit builder.
    let (nodes, _) = take_built();
    let mut scene = Scene {
        size: Size::new(width as f32, height as f32),
        clear: Color::new(0.13, 0.17, 0.23, 1.0),
        nodes,
    };
    scene.layout();

    match run_app(&title, width.max(1), height.max(1), Some(scene)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[gnative] runtime error: {e}");
            1
        }
    }
}

/// Jalankan aplikasi dinamis: build awal + rebuild setiap klik.
///
/// `build` = alamat fungsi `__gn_build_scene` dari binary (didaftarkan
/// `main` via `FuncAddr`). Runtime memanggilnya untuk frame awal dan
/// setiap dispatch klik (klik → method → state berubah → build ulang).
///
/// # Safety
/// `build` harus alamat fungsi `extern "C" fn()` valid dari binary.
#[no_mangle]
pub unsafe extern "C" fn gn_window_run_dynamic(build: *const ()) -> i32 {
    if build.is_null() {
        eprintln!("[gnative] build fn null — batal");
        return 1;
    }
    *build_fn().lock().expect("lock build") = Some(build as usize);
    let pending = pending_state().lock().expect("lock pending").take();
    let (title, (width, height)) = pending.unwrap_or_else(|| ("Gnative".to_string(), (800, 600)));

    let size = Size::new(width.max(1) as f32, height.max(1) as f32);
    let (scene, actions) = rebuild_scene(size);
    match crate::event_loop::run_app_dynamic(&title, width.max(1), height.max(1), scene, actions) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[gnative] runtime error: {e}");
            1
        }
    }
}

/// Daftarkan method komponen untuk dispatch klik.
///
/// # Safety
/// `comp`/`method` harus C string valid; `func` alamat fungsi
/// `extern "C" fn()` tanpa parameter dari binary.
#[no_mangle]
pub unsafe extern "C" fn gn_register_method(
    comp: *const c_char,
    method: *const c_char,
    func: *const (),
) {
    if comp.is_null() || method.is_null() || func.is_null() {
        return;
    }
    let c = unsafe { CStr::from_ptr(comp) }
        .to_string_lossy()
        .into_owned();
    let m = unsafe { CStr::from_ptr(method) }
        .to_string_lossy()
        .into_owned();
    methods()
        .lock()
        .expect("lock methods")
        .insert((c, m), func as usize);
}

/// Konfigurasi window yang dikirim `gn_window_create` sebelum `gn_window_run`.
type PendingWindow = Option<(String, (u32, u32))>;

static PENDING: OnceLock<Mutex<PendingWindow>> = OnceLock::new();

fn pending_state() -> &'static Mutex<PendingWindow> {
    PENDING.get_or_init(|| Mutex::new(None))
}

/// Rakitan scene yang sedang dibangun oleh builder intrinsik.
///
/// `stack` adalah tumpukan kontainer; elemen terakhir adalah kontainer aktif
/// tempat node baru ditambahkan. `begin_vstack` menekan kontainer baru,
/// `end_vstack` mengeluarkannya dan membungkusnya sebagai `StackNode`.
/// `next_id`/`actions` mencatat tombol klikabel (`id` → pemilik method)
/// untuk dispatch klik + rebuild.
struct SceneBuilder {
    stack: Vec<(f32, Vec<Node>)>,
    next_id: usize,
    actions: Vec<(usize, String, String)>,
}

static SCENE_BUILDER: OnceLock<Mutex<Option<SceneBuilder>>> = OnceLock::new();

fn scene_builder() -> &'static Mutex<Option<SceneBuilder>> {
    SCENE_BUILDER.get_or_init(|| Mutex::new(None))
}

/// Tabel method terregistrasi `(komponen, method)` → alamat fungsi binary.
/// Diisi `gn_register_method` dari `main` hasil kompiler sebelum run.
static METHODS: OnceLock<Mutex<HashMap<(String, String), usize>>> = OnceLock::new();

fn methods() -> &'static Mutex<HashMap<(String, String), usize>> {
    METHODS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Fungsi build scene (`__gn_build_scene` dari binary) untuk rebuild
/// pasca-klik. Diisi `gn_window_run_dynamic`.
static BUILD: OnceLock<Mutex<Option<usize>>> = OnceLock::new();

fn build_fn() -> &'static Mutex<Option<usize>> {
    BUILD.get_or_init(|| Mutex::new(None))
}

/// Tipe fungsi build + handler dari binary (ABI SystemV — sama seperti
/// fungsi Cranelift yang diekspor; tanpa parameter/return).
type BinaryFn = extern "C" fn();

/// Ambil hasil rakitan builder: node + aksi klik.
fn take_built() -> (Vec<Node>, Vec<(usize, String, String)>) {
    let mut b = scene_builder().lock().expect("lock builder");
    match b.take() {
        Some(builder) => {
            // Root = kontainer pertama (build seimbang → satu-satunya).
            let nodes = builder
                .stack
                .into_iter()
                .next()
                .map(|(_, n)| n)
                .unwrap_or_default();
            (nodes, builder.actions)
        }
        None => (Vec::new(), Vec::new()),
    }
}

/// Bangun ulang scene lewat fungsi build binary (dipakai frame awal dan
/// setiap dispatch klik). Me-layout dengan `size`.
pub(crate) fn rebuild_scene(size: Size) -> (Scene, Vec<(usize, String, String)>) {
    if let Some(ptr) = build_fn().lock().expect("lock build").as_ref().copied() {
        let f: BinaryFn = unsafe { std::mem::transmute(ptr) };
        f();
    }
    let (nodes, actions) = take_built();
    let mut scene = Scene {
        size,
        clear: Color::new(0.13, 0.17, 0.23, 1.0),
        nodes,
    };
    scene.layout();
    (scene, actions)
}

/// Panggil method terregistrasi; `true` bila ditemukan (pemanggil lalu rebuild).
pub(crate) fn call_method(comp: &str, method: &str) -> bool {
    let key = (comp.to_string(), method.to_string());
    let ptr = methods().lock().expect("lock methods").get(&key).copied();
    match ptr {
        Some(p) => {
            let f: BinaryFn = unsafe { std::mem::transmute(p) };
            f();
            true
        }
        None => false,
    }
}

/// Handle scene (selalu 1 — satu window per aplikasi).
const SCENE_HANDLE: WindowHandle = 1;

/// Buat scene kosong; kembalikan handle.
#[no_mangle]
pub extern "C" fn gn_scene_create() -> WindowHandle {
    let mut b = scene_builder().lock().expect("lock builder");
    *b = Some(SceneBuilder {
        stack: vec![(0.0, Vec::new())],
        next_id: 0,
        actions: Vec::new(),
    });
    SCENE_HANDLE
}

/// Tambah node ke kontainer aktif (paling atas tumpukan).
fn push_node(node: Node) {
    let mut b = scene_builder().lock().expect("lock builder");
    if let Some(b) = b.as_mut() {
        if let Some((_, children)) = b.stack.last_mut() {
            children.push(node);
        }
    }
}

/// `<text content={...} size={n}/>` → `Text`.
///
/// # Safety
/// `ptr` harus pointer valid ke C string (null-terminated).
#[no_mangle]
pub unsafe extern "C" fn gn_scene_add_text(_scene: WindowHandle, ptr: *const c_char, size: f64) {
    if ptr.is_null() {
        return;
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    push_node(Node::Text(TextItem {
        rect: Rect::new(0.0, 0.0, 0.0, 0.0),
        text,
        font_size: size as f32,
        color: Color::new(1.0, 1.0, 1.0, 1.0),
    }));
}

/// `<button label={...}/>` → `Group { Quad(bg) + Text(label) }`.
///
/// # Safety
/// `ptr` harus pointer valid ke C string (null-terminated).
#[no_mangle]
pub unsafe extern "C" fn gn_scene_add_button(_scene: WindowHandle, ptr: *const c_char) {
    if ptr.is_null() {
        return;
    }
    let label = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    push_node(Node::Group(vec![
        Node::Quad(
            Quad::new(
                Rect::new(0.0, 0.0, 220.0, 36.0),
                Color::new(0.2, 0.5, 0.85, 1.0),
            )
            .with_radius(8.0),
        ),
        Node::Text(TextItem {
            rect: Rect::new(12.0, 9.0, 0.0, 0.0),
            text: label,
            font_size: 14.0,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
        }),
    ]));
}

/// `<button label={...} onClick={this.m}/>` → `Action { Group {...} }`.
///
/// Sama seperti tombol biasa, dibungkus `Node::Action` + aksi dicatat
/// (`id` → pemilik method) untuk hit-test dan dispatch klik.
///
/// # Safety
/// `ptr`/`comp`/`method` harus pointer valid ke C string (null-terminated).
#[no_mangle]
pub unsafe extern "C" fn gn_scene_add_button_clickable(
    _scene: WindowHandle,
    ptr: *const c_char,
    comp: *const c_char,
    method: *const c_char,
) {
    if ptr.is_null() || comp.is_null() || method.is_null() {
        return;
    }
    let label = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    let c = unsafe { CStr::from_ptr(comp) }
        .to_string_lossy()
        .into_owned();
    let m = unsafe { CStr::from_ptr(method) }
        .to_string_lossy()
        .into_owned();
    let mut b = scene_builder().lock().expect("lock builder");
    if let Some(b) = b.as_mut() {
        let id = b.next_id;
        b.next_id += 1;
        b.actions.push((id, c, m));
        let group = Node::Group(vec![
            Node::Quad(
                Quad::new(
                    Rect::new(0.0, 0.0, 220.0, 36.0),
                    Color::new(0.2, 0.5, 0.85, 1.0),
                )
                .with_radius(8.0),
            ),
            Node::Text(TextItem {
                rect: Rect::new(12.0, 9.0, 0.0, 0.0),
                text: label,
                font_size: 14.0,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            }),
        ]);
        if let Some((_, children)) = b.stack.last_mut() {
            children.push(Node::Action {
                id,
                inner: Box::new(group),
            });
        }
    }
}

/// `<input placeholder={...}/>` → `Group { Quad(bg) + Text(placeholder) }`.
///
/// # Safety
/// `ptr` harus pointer valid ke C string (null-terminated).
#[no_mangle]
pub unsafe extern "C" fn gn_scene_add_input(_scene: WindowHandle, ptr: *const c_char) {
    if ptr.is_null() {
        return;
    }
    let placeholder = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    push_node(Node::Group(vec![
        Node::Quad(
            Quad::new(
                Rect::new(0.0, 0.0, 240.0, 36.0),
                Color::new(0.22, 0.26, 0.32, 1.0),
            )
            .with_radius(8.0),
        ),
        Node::Text(TextItem {
            rect: Rect::new(12.0, 9.0, 0.0, 0.0),
            text: placeholder,
            font_size: 14.0,
            color: Color::new(0.7, 0.74, 0.8, 1.0),
        }),
    ]));
}

/// Mulai `<vstack gap={n}>` — tekan kontainer baru ke tumpukan.
#[no_mangle]
pub extern "C" fn gn_scene_begin_vstack(_scene: WindowHandle, gap: f64) {
    let mut b = scene_builder().lock().expect("lock builder");
    if let Some(b) = b.as_mut() {
        b.stack.push((gap as f32, Vec::new()));
    }
}

/// Tutup `</vstack>` — keluarkan kontainer & bungkus jadi `StackNode`.
#[no_mangle]
pub extern "C" fn gn_scene_end_vstack(_scene: WindowHandle) {
    let mut b = scene_builder().lock().expect("lock builder");
    if let Some(b) = b.as_mut() {
        if b.stack.len() > 1 {
            let (gap, children) = b.stack.pop().expect("stack tidak kosong");
            let node = Node::Stack(StackNode {
                direction: StackDirection::Vertical,
                gap,
                children,
            });
            if let Some((_, top)) = b.stack.last_mut() {
                top.push(node);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_title_invalid() {
        unsafe {
            assert_eq!(
                gn_window_create(std::ptr::null(), 0, 800, 600),
                INVALID_HANDLE
            );
        }
    }

    #[test]
    fn create_sets_pending() {
        let c = std::ffi::CString::new("Halo").unwrap();
        unsafe {
            let h = gn_window_create(c.as_ptr(), 4, 1024, 768);
            assert_eq!(h, HANDLE_MAIN);
            let pending = pending_state().lock().unwrap();
            assert_eq!(pending.as_ref().unwrap().0, "Halo");
            assert_eq!(pending.as_ref().unwrap().1, (1024, 768));
        }
    }

    /// Siklus klik headless: build → hit-test → dispatch → rebuild.
    ///
    /// Nama komponen/method unik agar aman paralel dengan test lain.
    mod klik {
        use super::*;
        use std::sync::atomic::{AtomicI32, Ordering};

        static PANGGIL: AtomicI32 = AtomicI32::new(0);

        extern "C" fn build_uji() {
            unsafe {
                gn_scene_create();
                gn_scene_add_button_clickable(
                    SCENE_HANDLE,
                    c"+".as_ptr(),
                    c"KUji".as_ptr(),
                    c"naik".as_ptr(),
                );
            }
        }

        extern "C" fn handler_uji() {
            PANGGIL.fetch_add(1, Ordering::SeqCst);
        }

        fn daftar() {
            unsafe {
                gn_register_method(c"KUji".as_ptr(), c"naik".as_ptr(), handler_uji as *const ());
                *build_fn().lock().expect("lock build") = Some(build_uji as usize);
            }
        }

        #[test]
        fn build_menghasilkan_aksi() {
            daftar();
            let (scene, actions) = rebuild_scene(Size::new(800.0, 600.0));
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].1, "KUji");
            assert_eq!(actions[0].2, "naik");
            assert!(scene.nodes.iter().any(|n| matches!(n, Node::Action { .. })));
        }

        #[test]
        fn klik_tombol_dispatch() {
            daftar();
            let before = PANGGIL.load(Ordering::SeqCst);
            let (scene, actions) = rebuild_scene(Size::new(800.0, 600.0));
            // Tombol di (16,16)+padding layout, 220x36 → titik dalam kena.
            let hit = scene.action_at(&actions, 100.0, 30.0);
            assert_eq!(
                hit,
                Some(("KUji".to_string(), "naik".to_string())),
                "node: {:?}",
                scene.nodes
            );
            let (c, m) = hit.unwrap();
            assert!(call_method(&c, &m));
            assert_eq!(PANGGIL.load(Ordering::SeqCst), before + 1);
        }

        #[test]
        fn klik_kosong_meleset() {
            daftar();
            let (scene, actions) = rebuild_scene(Size::new(800.0, 600.0));
            assert_eq!(scene.action_at(&actions, 700.0, 500.0), None);
        }

        #[test]
        fn method_tak_kenal_gagal() {
            assert!(!call_method("TakAda", "tak_ada"));
        }
    }
}
