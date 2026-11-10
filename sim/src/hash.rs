//! FNV-1a 64-bit hashing for the state hash (SCOPE 6.10).
//!
//! Used only for deterministic regression detection; it is not cryptographic. Values are
//! written as explicit little-endian bytes, never as raw struct memory, so the result
//! does not depend on padding, pointers or `std` hasher seeds. This deliberately does not
//! implement `std::hash::Hasher`, whose `write_usize` would make results depend on the
//! platform's pointer width.

const OFFSET_BASIS: u64 = 0xCBF2_9CE4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01B3;

#[derive(Debug, Clone)]
pub(crate) struct Fnv1a64 {
    state: u64,
}

impl Fnv1a64 {
    pub(crate) fn new() -> Self {
        Self {
            state: OFFSET_BASIS,
        }
    }

    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= u64::from(byte);
            self.state = self.state.wrapping_mul(PRIME);
        }
    }

    pub(crate) fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    pub(crate) fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    pub(crate) fn write_u128(&mut self, value: u128) {
        self.write_bytes(&value.to_le_bytes());
    }

    /// Hashes the IEEE 754 bit pattern, little-endian, with `-0.0` written as `0.0`
    /// because the simulation treats them as equal. Values are never rounded. NaN and
    /// infinity are kept out of simulation state by validation, not handled here.
    pub(crate) fn write_f32(&mut self, value: f32) {
        let normalized = if value == 0.0 { 0.0 } else { value };
        self.write_u32(normalized.to_bits());
    }

    /// Hashes the IEEE 754 bit pattern of an `f64`, little-endian, with `-0.0` written as
    /// `0.0`, as for [`Fnv1a64::write_f32`].
    pub(crate) fn write_f64(&mut self, value: f64) {
        let normalized = if value == 0.0 { 0.0 } else { value };
        self.write_u64(normalized.to_bits());
    }

    pub(crate) fn write_u64_slice(&mut self, values: &[u64]) {
        for &value in values {
            self.write_u64(value);
        }
    }

    pub(crate) fn write_u32_slice(&mut self, values: &[u32]) {
        for &value in values {
            self.write_u32(value);
        }
    }

    pub(crate) fn write_f32_slice(&mut self, values: &[f32]) {
        for &value in values {
            self.write_f32(value);
        }
    }

    pub(crate) fn finish(&self) -> u64 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn f32_is_hashed_as_bits_with_negative_zero_normalized() {
        let hash = |value: f32| {
            let mut hasher = Fnv1a64::new();
            hasher.write_f32(value);
            hasher.finish()
        };
        assert_eq!(hash(-0.0), hash(0.0));
        assert_eq!(hash(1.5), hash_bytes(&1.5_f32.to_bits().to_le_bytes()));
        assert_ne!(hash(1.0), hash(f32::from_bits(1.0_f32.to_bits() + 1)));
    }

    use super::*;

    fn hash_bytes(bytes: &[u8]) -> u64 {
        let mut hasher = Fnv1a64::new();
        hasher.write_bytes(bytes);
        hasher.finish()
    }

    /// Published FNV-1a 64-bit test vectors.
    #[test]
    fn matches_reference_vectors() {
        assert_eq!(hash_bytes(b""), 0xCBF2_9CE4_8422_2325);
        assert_eq!(hash_bytes(b"a"), 0xAF63_DC4C_8601_EC8C);
        assert_eq!(hash_bytes(b"foobar"), 0x8594_4171_F739_67E8);
    }

    #[test]
    fn integers_are_hashed_little_endian() {
        let mut hasher = Fnv1a64::new();
        hasher.write_u32(0x0403_0201);
        hasher.write_u64(0x0C0B_0A09_0807_0605);
        assert_eq!(
            hasher.finish(),
            hash_bytes(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12])
        );
    }
}
