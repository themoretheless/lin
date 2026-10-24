fn reserve_wal_space(log: &File, additional: usize, empty_only: bool) {
    #[cfg(target_vendor = "apple")]
    {
        use std::os::unix::{fs::MetadataExt, io::AsRawFd};
        const CHUNK: u64 = 1024 * 1024;
        let Ok(metadata) = log.metadata() else {
            return;
        };
        if empty_only && metadata.len() != 0 {
            return;
        }
        let Ok(additional) = u64::try_from(additional) else {
            return;
        };
        let Some(required) = metadata.len().checked_add(additional) else {
            return;
        };
        let allocated = metadata.blocks().saturating_mul(512);
        if allocated >= required {
            return;
        }
        let Some(rounded) = required.checked_add(CHUNK - 1).map(|n| n / CHUNK * CHUNK) else {
            return;
        };
        // Do not reserve an arbitrarily large sparse-file backlog.
        let length = rounded
            .saturating_sub(allocated)
            .min(additional.saturating_add(CHUNK));
        let Ok(length) = i64::try_from(length) else {
            return;
        };
        if length == 0 {
            return;
        }
        let mut request = libc::fstore_t {
            fst_flags: libc::F_ALLOCATEALL,
            fst_posmode: libc::F_PEOFPOSMODE,
            fst_offset: 0,
            fst_length: length,
            fst_bytesalloc: 0,
        };
        // Unsupported filesystem / allocation failure retains the ordinary write path.
        let _ = unsafe { libc::fcntl(log.as_raw_fd(), libc::F_PREALLOCATE, &mut request) };
    }
    #[cfg(not(target_vendor = "apple"))]
    let _ = (log, additional, empty_only);
}
