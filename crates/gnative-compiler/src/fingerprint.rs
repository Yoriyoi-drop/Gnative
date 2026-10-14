//! Fingerprint: hash deterministik source untuk incremental build.

/// Hash FNV-1a 64-bit — cepat, deterministik, tanpa dependency.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Hash source file `.gn`.
pub fn source_fingerprint(source: &str) -> u64 {
    fnv1a(source.as_bytes())
}

/// Representasi hex.
pub fn to_hex(hash: u64) -> String {
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(source_fingerprint("hello"), source_fingerprint("hello"));
        assert_ne!(source_fingerprint("hello"), source_fingerprint("hello!"));
    }
}
