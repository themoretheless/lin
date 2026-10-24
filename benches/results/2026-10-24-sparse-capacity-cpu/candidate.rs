fn encode_pointer(
    rows: &[Option<std::sync::Arc<[f32]>>],
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    for row in rows {
        let Some(emb) = row else {
            buf.extend_from_slice(&0u32.to_le_bytes());
            continue;
        };
        let dim = u32::try_from(emb.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| io_err("sparse vector dimension overflow"))?;
        buf.extend_from_slice(&dim.to_le_bytes());
        let count_offset = buf.len();
        buf.extend_from_slice(&0u32.to_le_bytes());
        let mut count = 0u32;
        for (block, values) in emb.chunks(8).enumerate() {
            // Bitwise zero preserves negative zero and NaN payloads.
            // Skip empty blocks without branching for each element.
            if values
                .iter()
                .fold(0u32, |bits, value| bits | value.to_bits())
                == 0
            {
                continue;
            }
            // At most values.len() eight-byte entries; reserve before taking pointer.
            buf.reserve(values.len() * 8);
            let start = buf.len();
            let mut written = 0usize;
            let out = buf.as_mut_ptr();
            for (offset, value) in values.iter().enumerate() {
                let bits = value.to_bits();
                if bits != 0 {
                    let index = (block * 8 + offset) as u32;
                    let entry = ((bits as u64) << 32) | index as u64;
                    // All writes fit reserved spare capacity; no allocation until set_len.
                    unsafe { std::ptr::write_unaligned(out.add(start + written) as *mut u64, entry.to_le()); }
                    written += 8;
                    count += 1;
                }
            }
            // Exactly written bytes were initialized above, including empty blocks.
            unsafe { buf.set_len(start + written); }
        }
        buf[count_offset..count_offset + 4].copy_from_slice(&count.to_le_bytes());
    }
    Ok(())
}
