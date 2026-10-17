#![no_main]
use libfuzzer_sys::fuzz_target;
use gnative_core::safe_join;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let mid = data.len() / 2;
        let left = String::from_utf8_lossy(&data[..mid]).to_string();
        let right = String::from_utf8_lossy(&data[mid..]).to_string();
        let root = Path::new(&left);
        let _ = safe_join(root, &right);
        let _ = safe_join(Path::new("/tmp/root"), &s);
    }
    let _ = safe_join(Path::new("/tmp/root"), "");
});
