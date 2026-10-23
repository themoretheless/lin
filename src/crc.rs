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
    let mut chunks = data.chunks_exact(8);
    for chunk in &mut chunks {
        let val = u64::from_le_bytes(chunk.try_into().unwrap());
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
    let mut chunks = data.chunks_exact(8);
    let mut crc64 = crc as u64;
    for chunk in &mut chunks {
        let val = u64::from_le_bytes(chunk.try_into().unwrap());
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
}
