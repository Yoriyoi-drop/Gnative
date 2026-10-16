//! Penyimpanan state komponen — dipanggil binary `.gn` via C-ABI.
//!
//! Komponen stateful (`.gn`: `state { count: number = 0; }`) di-lower
//! kompiler menjadi panggilan `gn_state_set_*` (init + mutasi) dan
//! `gn_state_get_*` (render + method). Satu instance per tipe komponen
//! (kunci = `(komponen, field)`; multi-instance = roadmap).
//!
//! Nilai string **disalin** ke dalam store saat di-set, sehingga pointer yang
//! dikembalikan `gn_state_get_str` selalu valid selama entri hidup (P2-13 —
//! tidak lagi menyimpan pointer pool mentah yang bisa dangling).
//! Jumlah entri dibatasi `MAX_STATE_ENTRIES` agar tak tumbuh tanpa batas.

use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

/// Batas jumlah entri state (kunci) — cegah pertumbuhan tanpa batas (P2-13).
const MAX_STATE_ENTRIES: usize = 4096;

/// Nilai state satu field (semua slot; yang dipakai tergantung tipe).
#[derive(Debug, Clone, Default)]
struct StateVal {
    f64_val: f64,
    bool_val: bool,
    /// String milik store (di-copy saat set) → pointer stabil selama hidup.
    str_val: Option<CString>,
}

static STORE: OnceLock<Mutex<HashMap<(String, String), StateVal>>> = OnceLock::new();

fn store() -> &'static Mutex<HashMap<(String, String), StateVal>> {
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Akses satu entri state dengan batas jumlah entri (P2-13).
///
/// Memulihkan lock yang ter-poison (seragam dengan `intrinsics::locked`).
/// Bila entri baru melebihi `MAX_STATE_ENTRIES`, operasi diabaikan.
fn with_entry<F: FnOnce(&mut StateVal)>(key: (String, String), f: F) {
    let mut s = crate::intrinsics::locked(store());
    if !s.contains_key(&key) && s.len() >= MAX_STATE_ENTRIES {
        return;
    }
    f(s.entry(key).or_default());
}

/// Baca pasangan C string → `(komponen, field)`; `None` bila null.
/// Dibaca dengan batas eksplisit (P0-08) — tidak mengandalkan NUL tak terbatas.
unsafe fn key_of(comp: *const c_char, field: *const c_char) -> Option<(String, String)> {
    if comp.is_null() || field.is_null() {
        return None;
    }
    let c = crate::intrinsics::read_cstr(comp, 0);
    let f = crate::intrinsics::read_cstr(field, 0);
    Some((c, f))
}

/// Tulis nilai `number` (f64).
///
/// # Safety
/// `comp`/`field` harus C string valid atau null (null → no-op).
#[no_mangle]
pub unsafe extern "C" fn gn_state_set_f64(comp: *const c_char, field: *const c_char, val: f64) {
    let Some(key) = key_of(comp, field) else {
        return;
    };
    with_entry(key, |v| v.f64_val = val);
}

/// Baca nilai `number`; belum di-init → `0.0`.
///
/// # Safety
/// `comp`/`field` harus C string valid atau null (null → `0.0`).
#[no_mangle]
pub unsafe extern "C" fn gn_state_get_f64(comp: *const c_char, field: *const c_char) -> f64 {
    let Some(key) = key_of(comp, field) else {
        return 0.0;
    };
    crate::intrinsics::locked(store())
        .get(&key)
        .map(|v| v.f64_val)
        .unwrap_or(0.0)
}

/// Tulis nilai `boolean` (0/1).
///
/// # Safety
/// `comp`/`field` harus C string valid atau null (null → no-op).
#[no_mangle]
pub unsafe extern "C" fn gn_state_set_bool(comp: *const c_char, field: *const c_char, val: i8) {
    let Some(key) = key_of(comp, field) else {
        return;
    };
    with_entry(key, |v| v.bool_val = val != 0);
}

/// Baca nilai `boolean`; belum di-init → `0`.
///
/// # Safety
/// `comp`/`field` harus C string valid atau null (null → `0`).
#[no_mangle]
pub unsafe extern "C" fn gn_state_get_bool(comp: *const c_char, field: *const c_char) -> i8 {
    let Some(key) = key_of(comp, field) else {
        return 0;
    };
    i8::from(
        crate::intrinsics::locked(store())
            .get(&key)
            .is_some_and(|v| v.bool_val),
    )
}

/// Tulis nilai `string` (C string; disalin ke store; null → diabaikan).
///
/// # Safety
/// `comp`/`field`/`ptr` harus C string valid atau null (null → no-op).
#[no_mangle]
pub unsafe extern "C" fn gn_state_set_str(
    comp: *const c_char,
    field: *const c_char,
    ptr: *const c_char,
) {
    if ptr.is_null() {
        return;
    }
    let Some(key) = key_of(comp, field) else {
        return;
    };
    let text = crate::intrinsics::read_cstr(ptr, 0);
    // Salin jadi CString milik store (interior NUL tak mungkin dari read_cstr).
    let Ok(owned) = CString::new(text) else {
        return;
    };
    with_entry(key, |v| v.str_val = Some(owned));
}

/// Baca nilai `string`; belum di-init → null.
///
/// # Safety
/// `comp`/`field` harus C string valid atau null (null → null).
/// Pointer hasil milik store (jangan di-free).
#[no_mangle]
pub unsafe extern "C" fn gn_state_get_str(
    comp: *const c_char,
    field: *const c_char,
) -> *const c_char {
    let Some(key) = key_of(comp, field) else {
        return std::ptr::null();
    };
    crate::intrinsics::locked(store())
        .get(&key)
        .and_then(|v| v.str_val.as_ref())
        .map(|c| c.as_ptr())
        .unwrap_or(std::ptr::null())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn key(c: &str, f: &str) -> (CString, CString) {
        (CString::new(c).unwrap(), CString::new(f).unwrap())
    }

    #[test]
    fn f64_roundtrip_dan_default() {
        let (c, f) = key("T_F64", "n");
        unsafe {
            assert_eq!(gn_state_get_f64(c.as_ptr(), f.as_ptr()), 0.0);
            gn_state_set_f64(c.as_ptr(), f.as_ptr(), 42.5);
            assert_eq!(gn_state_get_f64(c.as_ptr(), f.as_ptr()), 42.5);
        }
    }

    #[test]
    fn bool_roundtrip() {
        let (c, f) = key("T_BOOL", "b");
        unsafe {
            assert_eq!(gn_state_get_bool(c.as_ptr(), f.as_ptr()), 0);
            gn_state_set_bool(c.as_ptr(), f.as_ptr(), 1);
            assert_eq!(gn_state_get_bool(c.as_ptr(), f.as_ptr()), 1);
            gn_state_set_bool(c.as_ptr(), f.as_ptr(), 0);
            assert_eq!(gn_state_get_bool(c.as_ptr(), f.as_ptr()), 0);
        }
    }

    #[test]
    fn str_roundtrip_dan_null_default() {
        let (c, f) = key("T_STR", "s");
        let v = CString::new("halo").unwrap();
        unsafe {
            assert!(gn_state_get_str(c.as_ptr(), f.as_ptr()).is_null());
            gn_state_set_str(c.as_ptr(), f.as_ptr(), v.as_ptr());
            let got = gn_state_get_str(c.as_ptr(), f.as_ptr());
            assert_eq!(crate::intrinsics::read_cstr(got, 0), "halo");
        }
    }

    #[test]
    fn str_disalin_bukan_pointer_sumber() {
        // P2-13: string di-copy; pointer sumber boleh mati, isi state tetap ada.
        let (c, f) = key("T_OWN", "s");
        let ptr = {
            let tmp = CString::new("sementara").unwrap();
            unsafe {
                gn_state_set_str(c.as_ptr(), f.as_ptr(), tmp.as_ptr());
            }
            tmp.as_ptr()
        }; // `tmp` di-drop → pointer sumber dangling
        let _ = ptr; // hanya untuk menegaskan sumber tak dipakai lagi
        unsafe {
            let got = gn_state_get_str(c.as_ptr(), f.as_ptr());
            assert_eq!(crate::intrinsics::read_cstr(got, 0), "sementara");
        }
    }

    #[test]
    fn null_tidak_panic() {
        unsafe {
            gn_state_set_f64(std::ptr::null(), std::ptr::null(), 1.0);
            assert_eq!(gn_state_get_f64(std::ptr::null(), std::ptr::null()), 0.0);
            assert!(gn_state_get_str(std::ptr::null(), std::ptr::null()).is_null());
        }
    }
}
