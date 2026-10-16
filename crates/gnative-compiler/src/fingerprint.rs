//! Fingerprint: hash deterministik source untuk incremental build.

/// Hash FNV-1a 64-bit — cepat, deterministik, tanpa dependency.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    fnv1a_seeded(0xcbf2_9ce4_8422_2325, bytes)
}

/// FNV-1a dengan seed awal (basis offset diganti `seed`) — untuk
/// menggabungkan beberapa sumber (versi + mode + konten banyak file) ke
/// dalam satu fingerprint tanpa alokasi buffer besar.
pub fn fnv1a_seeded(mut hash: u64, bytes: &[u8]) -> u64 {
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
