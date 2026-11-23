//! FNV-1a, a hash that stays the same across platforms and compiler versions.
//!
//! It names the toolbox image and each project's cache volumes. It is not a security
//! boundary: nothing an attacker controls is hashed against a victim's value.

/// Offset basis of 64-bit FNV-1a.
const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// Prime of 64-bit FNV-1a.
const PRIME: u64 = 0x0000_0100_0000_01b3;
/// Hex digits kept in a name: 48 bits, short enough to read in `docker image ls`.
const SHORT_LEN: usize = 12;

#[derive(Debug, Clone, Copy)]
pub struct Fnv(u64);

impl Fnv {
    pub fn new() -> Self {
        Self(OFFSET)
    }

    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(PRIME);
        }
    }

    pub fn short_hex(self) -> String {
        let mut hex = format!("{:016x}", self.0);
        hex.truncate(SHORT_LEN);
        hex
    }
}

/// The short fingerprint of one byte string.
pub fn of(bytes: &[u8]) -> String {
    let mut hash = Fnv::new();
    hash.update(bytes);
    hash.short_hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_published_fnv1a_vectors() {
        let full = |text: &str| {
            let mut hash = Fnv::new();
            hash.update(text.as_bytes());
            hash.0
        };
        assert_eq!(full(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(full("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(full("foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn short_form_is_the_leading_hex_digits() {
        assert_eq!(of(b"foobar"), "85944171f739");
    }
}
