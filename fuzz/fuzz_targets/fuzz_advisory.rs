#![no_main]
use libfuzzer_sys::fuzz_target;
use gnative_compiler::advisory::{AdvisoryDb, compare_versions, satisfies};

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = compare_versions(s, s);
        let _ = satisfies(">=", s);
    }
    // Load arbitrary advisory DB text; should not panic.
    if let Ok(toml_txt) = std::str::from_utf8(data) {
        if let Ok(db) = AdvisoryDb::parse(toml_txt) {
            let _ = db.len();
        }
    }
});
