//! Penyimpanan state komponen — dipanggil binary `.gn` via C-ABI.
//!
//! Komponen stateful (`.gn`: `state { count: number = 0; }`) di-lower
//! kompiler menjadi panggilan `gn_state_set_*` (init + mutasi) dan
//! `gn_state_get_*` (render + method). Satu instance per tipe komponen
//! (kunci = `(komponen, field)`; multi-instance = roadmap).
//!
//! Nilai string disimpan sebagai pointer ke pool string binary (hidup
//! sepanjang proses, seperti `gn_scene_add_text`) — tidak di-copy.

use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

/// Nilai state satu field (semua slot; yang dipakai tergantung tipe).
#[derive(Debug, Clone, Copy, Default)]
struct StateVal {
    f64_val: f64,
    bool_val: bool,
    str_ptr: usize,
}

static STORE: OnceLock<Mutex<HashMap<(String, String), StateVal>>> = OnceLock::new();

fn store() -> &'static Mutex<HashMap<(String, String), StateVal>> {
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Baca pasangan C string → `(komponen, field)`; `None` bila null.
fn key_of(comp: *const c_char, field: *const c_char) -> Option<(String, String)> {
    if comp.is_null() || field.is_null() {
        return None;
    }
    let c = unsafe { CStr::from_ptr(comp) }
        .to_string_lossy()
        .into_owned();
    let f = unsafe { CStr::from_ptr(field) }
        .to_string_lossy()
        .into_owned();
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
    if let Ok(mut s) = store().lock() {
        s.entry(key).or_default().f64_val = val;
    }
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
    store()
        .lock()
        .map(|s| s.get(&key).map(|v| v.f64_val).unwrap_or(0.0))
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
    if let Ok(mut s) = store().lock() {
        s.entry(key).or_default().bool_val = val != 0;
    }
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
    store()
        .lock()
        .map(|s| i8::from(s.get(&key).is_some_and(|v| v.bool_val)))
        .unwrap_or(0)
}

/// Tulis nilai `string` (pointer C string pool binary; null → diabaikan).
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
    if let Ok(mut s) = store().lock() {
        s.entry(key).or_default().str_ptr = ptr as usize;
    }
}

/// Baca nilai `string`; belum di-init → null.
///
/// # Safety
/// `comp`/`field` harus C string valid atau null (null → null).
/// Pointer hasil milik pool binary (jangan di-free).
#[no_mangle]
pub unsafe extern "C" fn gn_state_get_str(
    comp: *const c_char,
    field: *const c_char,
) -> *const c_char {
    let Some(key) = key_of(comp, field) else {
        return std::ptr::null();
    };
    store()
        .lock()
        .map(|s| {
            s.get(&key)
                .map(|v| v.str_ptr as *const c_char)
                .unwrap_or(std::ptr::null())
        })
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
            assert_eq!(CStr::from_ptr(got).to_string_lossy(), "halo");
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
