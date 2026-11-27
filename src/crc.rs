//! Hardware-accelerated Castagnoli CRC32-C (`0x82F63B78` / `0x1EDC6F41`).
//!
//! Uses dedicated CPU instructions when available:
//! - ARMv8-A: `__crc32cb` / `__crc32cd` (Apple Silicon M-series, AWS Graviton, modern ARM)
//! - x86_64: SSE4.2 `_mm_crc32_u8` / `_mm_crc32_u64` (Intel Nehalem+, AMD Bulldozer+)
//! - Portable table-based fallback for platforms without hardware CRC instructions.

/// Castagnoli CRC-32C lookup table (polynomial `0x82F63B78` in reversed bit representation).
const CRC32C_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0x82F63B78;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

// Multiplication by x^(256 * 8) in the reflected CRC-32C field.
const LANE_BYTES: usize = 256;
const SHIFT_LANE: [[u32; 256]; 4] = {
    let mut table = [[0; 256]; 4];
    let mut byte = 0;
    while byte < 4 {
        let mut value = 0;
        while value < 256 {
            let mut crc = (value as u32) << (byte * 8);
            let mut n = 0;
            while n < LANE_BYTES {
                crc = CRC32C_TABLE[(crc & 255) as usize] ^ (crc >> 8);
                n += 1;
            }
            table[byte][value] = crc;
            value += 1;
        }
        byte += 1;
    }
    table
};

#[inline]
fn shift_lane(crc: u32) -> u32 {
    let [a, b, c, d] = crc.to_le_bytes();
    SHIFT_LANE[0][a as usize]
        ^ SHIFT_LANE[1][b as usize]
        ^ SHIFT_LANE[2][c as usize]
        ^ SHIFT_LANE[3][d as usize]
}

/// Portable software fallback using precomputed Castagnoli table.
#[inline]
pub fn crc32c_table(mut crc: u32, data: &[u8]) -> u32 {
    for &b in data {
        crc = CRC32C_TABLE[((crc ^ (b as u32)) & 0xff) as usize] ^ (crc >> 8);
    }
    crc
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "crc")]
unsafe fn crc32c_aarch64(mut crc: u32, data: &[u8]) -> u32 {
    use core::arch::aarch64::*;
    let mut groups = data.chunks_exact(3 * LANE_BYTES);
    for group in &mut groups {
        let (mut a, mut b, mut c) = (crc as u32, 0 as u32, 0 as u32);
        for offset in (0..LANE_BYTES).step_by(8) {
            let word =
                |start: usize| u64::from_le_bytes(group[start..start + 8].try_into().unwrap());
            a = __crc32cd(a, word(offset));
            b = __crc32cd(b, word(LANE_BYTES + offset));
            c = __crc32cd(c, word(2 * LANE_BYTES + offset));
        }
        crc = shift_lane(shift_lane(a as u32) ^ b as u32) ^ c as u32;
    }
    let data = groups.remainder();
    let mut chunks = data.chunks_exact(8);
    for chunk in &mut chunks {
        let &[b0, b1, b2, b3, b4, b5, b6, b7] = chunk else {
            unreachable!()
        };
        let val = u64::from_le_bytes([b0, b1, b2, b3, b4, b5, b6, b7]);
        crc = __crc32cd(crc, val);
    }
    let rem = chunks.remainder();
    for &b in rem {
        crc = __crc32cb(crc, b);
    }
    crc
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.2")]
unsafe fn crc32c_x86_64(mut crc: u32, data: &[u8]) -> u32 {
    use core::arch::x86_64::*;
    let mut groups = data.chunks_exact(3 * LANE_BYTES);
    for group in &mut groups {
        let (mut a, mut b, mut c) = (crc as u64, 0 as u64, 0 as u64);
        for offset in (0..LANE_BYTES).step_by(8) {
            let word =
                |start: usize| u64::from_le_bytes(group[start..start + 8].try_into().unwrap());
            a = _mm_crc32_u64(a, word(offset));
            b = _mm_crc32_u64(b, word(LANE_BYTES + offset));
            c = _mm_crc32_u64(c, word(2 * LANE_BYTES + offset));
        }
        crc = shift_lane(shift_lane(a as u32) ^ b as u32) ^ c as u32;
    }
    let data = groups.remainder();
    let mut chunks = data.chunks_exact(8);
    let mut crc64 = crc as u64;
    for chunk in &mut chunks {
        let &[b0, b1, b2, b3, b4, b5, b6, b7] = chunk else {
            unreachable!()
        };
        let val = u64::from_le_bytes([b0, b1, b2, b3, b4, b5, b6, b7]);
        crc64 = _mm_crc32_u64(crc64, val);
    }
    crc = crc64 as u32;
    let rem = chunks.remainder();
    for &b in rem {
        crc = _mm_crc32_u8(crc, b);
    }
    crc
}

/// Incremental CRC32-C hasher with hardware CPU acceleration where available.
#[derive(Clone, Copy, Debug)]
pub struct Hasher(u32);

impl Default for Hasher {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    #[inline]
    pub fn new() -> Self {
        Self(!0u32)
    }

    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        #[cfg(target_arch = "aarch64")]
        {
            #[cfg(target_os = "macos")]
            let has_crc = true;
            #[cfg(not(target_os = "macos"))]
            let has_crc = std::arch::is_aarch64_feature_detected!("crc");

            if has_crc {
                self.0 = unsafe { crc32c_aarch64(self.0, data) };
                return;
            }
        }
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("sse4.2") {
                self.0 = unsafe { crc32c_x86_64(self.0, data) };
                return;
            }
        }
        self.0 = crc32c_table(self.0, data);
    }

    #[inline]
    pub fn finalize(self) -> u32 {
        !self.0
    }
}

/// Compute 32-bit Castagnoli CRC32-C checksum with hardware CPU acceleration where available.
///
/// Returns standard Castagnoli CRC32-C with initial `0xFFFFFFFF` and final XOR `0xFFFFFFFF`.
#[inline]
pub fn hardware_crc32c(data: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_boundaries_match_serial_reference() {
        let data: Vec<u8> = (0..8192u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 24) as u8)
            .collect();
        for n in [767, 768, 769, 1535, 1536, 1537, 8192] {
            for prefix in [0, 1, 7, 256] {
                let mut hasher = Hasher::new();
                hasher.update(&data[..prefix]);
                hasher.update(&data[prefix..n]);
                assert_eq!(hasher.finalize(), !crc32c_table(!0, &data[..n]));
            }
        }
    }

    #[test]
    fn test_crc32c_standard_vector() {
        // Standard Castagnoli CRC-32C vector: "123456789" -> 0xE3069283
        let data = b"123456789";
        let table_res = !crc32c_table(!0u32, data);
        assert_eq!(table_res, 0xE3069283, "table CRC32-C mismatch");

        let hw_res = hardware_crc32c(data);
        assert_eq!(hw_res, 0xE3069283, "hardware CRC32-C mismatch");

        let mut h = Hasher::new();
        h.update(b"1234");
        h.update(b"56789");
        assert_eq!(h.finalize(), 0xE3069283, "incremental Hasher mismatch");
    }

    #[test]
    fn test_crc32c_various_lengths_and_alignments() {
        for len in 0..128 {
            let buffer: Vec<u8> = (0..len).map(|i| (i * 31 + 7) as u8).collect();
            let table_crc = !crc32c_table(!0u32, &buffer);
            let hw_crc = hardware_crc32c(&buffer);
            assert_eq!(table_crc, hw_crc, "mismatch at length {len}");
        }
    }

    #[test]
    fn test_crc32c_detects_bit_flips() {
        let original = b"The quick brown fox jumps over the lazy dog";
        let base_crc = hardware_crc32c(original);
        for i in 0..original.len() {
            let mut corrupted = original.to_vec();
            corrupted[i] ^= 1;
            assert_ne!(
                hardware_crc32c(&corrupted),
                base_crc,
                "missed bit flip at {i}"
            );
        }
    }

    /// The checksum of no bytes, in every form the WAL can ask for it.
    #[test]
    fn test_crc32c_empty_input() {
        assert_eq!(hardware_crc32c(&[]), 0);
        assert_eq!(!crc32c_table(!0u32, &[]), 0);
        let mut h = Hasher::new();
        h.update(&[]);
        h.update(&[]);
        assert_eq!(h.finalize(), 0);
        // Empty updates between real ones change nothing.
        let mut h = Hasher::new();
        h.update(&[]);
        h.update(b"1234");
        h.update(&[]);
        h.update(b"56789");
        h.update(&[]);
        assert_eq!(h.finalize(), 0xE3069283);
    }

    fn pattern(len: usize) -> Vec<u8> {
        let mut x = 0x9E37_79B9u32;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 24) as u8
            })
            .collect()
    }

    /// Feeding the data in pieces gives the checksum of the whole, wherever it is cut:
    /// a cut inside an 8-byte word must not change how the remaining bytes are folded.
    #[test]
    fn test_crc32c_segmented_updates_match_one_shot() {
        let data = pattern(203);
        let whole = hardware_crc32c(&data);
        assert_eq!(whole, !crc32c_table(!0u32, &data));
        for a in 0..=data.len() {
            let mut h = Hasher::new();
            h.update(&data[..a]);
            h.update(&data[a..]);
            assert_eq!(h.finalize(), whole, "split at {a}");
        }
        for a in (0..=data.len()).step_by(7) {
            for b in (a..=data.len()).step_by(5) {
                let mut h = Hasher::new();
                h.update(&data[..a]);
                h.update(&data[a..b]);
                h.update(&data[b..]);
                assert_eq!(h.finalize(), whole, "splits at {a} and {b}");
            }
        }
        // One byte at a time, software and accelerated alike.
        let mut h = Hasher::new();
        let mut table = !0u32;
        for byte in &data {
            h.update(std::slice::from_ref(byte));
            table = crc32c_table(table, std::slice::from_ref(byte));
        }
        assert_eq!(h.finalize(), whole);
        assert_eq!(!table, whole);
    }

    /// The accelerated path reads 8-byte words from wherever the slice starts; the
    /// result must not depend on the slice's alignment.
    #[test]
    fn test_crc32c_unaligned_slices() {
        let data = pattern(16 + 300);
        for offset in 0..16 {
            for len in [0, 1, 7, 8, 9, 15, 16, 17, 63, 64, 65, 255, 300] {
                let slice = &data[offset..offset + len];
                // Same bytes in a fresh allocation, i.e. at the allocator's alignment.
                let copy = slice.to_vec();
                let want = !crc32c_table(!0u32, &copy);
                assert_eq!(hardware_crc32c(slice), want, "offset {offset}, len {len}");
                assert_eq!(hardware_crc32c(&copy), want, "offset {offset}, len {len}");
            }
        }
    }

    /// Long inputs with every possible tail after the last whole word: the accelerated
    /// path and the software fallback agree, so a frame checksummed on one machine
    /// verifies on a machine without the instruction.
    #[test]
    fn test_crc32c_long_inputs_match_software_fallback() {
        let data = pattern((1 << 20) + 7);
        for tail in 0..8 {
            let slice = &data[..(1 << 20) + tail];
            assert_eq!(
                hardware_crc32c(slice),
                !crc32c_table(!0u32, slice),
                "tail {tail}"
            );
        }
        // A flipped bit in the tail bytes is still detected.
        let mut corrupt = data.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 0x80;
        assert_ne!(hardware_crc32c(&corrupt), hardware_crc32c(&data));
    }
}
