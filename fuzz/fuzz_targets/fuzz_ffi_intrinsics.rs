#![no_main]
use libfuzzer_sys::fuzz_target;
use gnative_runtime::intrinsics::{
    gn_register_method, gn_scene_add_button, gn_scene_add_button_clickable,
    gn_scene_add_input, gn_scene_add_text, gn_scene_begin_vstack, gn_scene_end_vstack,
};
use gnative_runtime::state_store::{
    gn_state_get_bool, gn_state_get_f64, gn_state_get_str, gn_state_set_bool, gn_state_set_f64,
    gn_state_set_str,
};
use std::os::raw::c_char;

/// Fungsi noop valid untuk daftar method (tidak null, bisa di-transmute).
extern "C" fn noop_handler() {}

/// Ambil slice data sebagai *const c_char (boleh null bila data kosong).
fn as_cstr_ptr(data: &[u8], offset: usize) -> *const c_char {
    if data.is_empty() || offset >= data.len() {
        return std::ptr::null();
    }
    // Tidak menambahkan NUL sengaja: uji read_cstr dengan buffer tak null-terminated.
    data[offset..].as_ptr() as *const c_char
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }

    // Panggil semua scene builder intrinsik dengan pointer/byte acak dari data.
    // read_cstr memiliki batas MAX_CSTR_LEN, jadi OOB read tidak boleh terjadi.
    unsafe {
        gn_scene_add_text(1, as_cstr_ptr(data, 0), data[0] as f64);
        gn_scene_add_button(1, as_cstr_ptr(data, 1));
        gn_scene_add_input(1, as_cstr_ptr(data, 2));
        gn_scene_add_button_clickable(
            1,
            as_cstr_ptr(data, 3),
            as_cstr_ptr(data, 0),
            as_cstr_ptr(data, 1),
        );
        gn_scene_begin_vstack(1, data[2] as f64);
        gn_scene_end_vstack(1);

        // Daftarkan method dengan fungsi noop valid (pointer non-null).
        gn_register_method(
            as_cstr_ptr(data, 0),
            as_cstr_ptr(data, 1),
            noop_handler as *const (),
        );

        // State store: set + get semua tipe.
        let key_c = as_cstr_ptr(data, 0);
        let key_f = as_cstr_ptr(data, 1);
        gn_state_set_f64(key_c, key_f, data[3] as f64);
        gn_state_set_bool(key_c, key_f, data[3] as i8);
        gn_state_set_str(key_c, key_f, as_cstr_ptr(data, 2));

        let _ = gn_state_get_f64(key_c, key_f);
        let _ = gn_state_get_bool(key_c, key_f);
        let _ = gn_state_get_str(key_c, key_f);
    }
});
