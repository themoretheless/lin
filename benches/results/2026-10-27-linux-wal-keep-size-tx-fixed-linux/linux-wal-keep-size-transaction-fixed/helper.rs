#[cfg(any(target_os = "linux", test))]
fn linux_wal_reservation_range(
    logical: u64,
    allocated: u64,
    additional: u64,
    empty_only: bool,
) -> Option<(i64, i64)> {
    const CHUNK: u64 = 1024 * 1024;
    if additional == 0 || additional > u64::from(MAX_RECORD) + 13 || (empty_only && logical != 0) {
        return None;
    }
    let required = logical.checked_add(additional)?;
    if allocated >= required {
        return None;
    }
    let length = additional.checked_add(CHUNK - 1)?.checked_div(CHUNK)?.checked_mul(CHUNK)?;
    let end = logical.checked_add(length)?;
    if end > i64::MAX as u64 {
        return None;
    }
    Some((i64::try_from(logical).ok()?, i64::try_from(length).ok()?))
}

#[cfg(target_os = "linux")]
fn try_reserve_wal_space_linux(
    log: &File,
    additional: usize,
    empty_only: bool,
) -> io::Result<bool> {
    use std::os::unix::{fs::MetadataExt, io::AsRawFd};
    let metadata = log.metadata()?;
    let Ok(additional) = u64::try_from(additional) else {
        return Ok(false);
    };
    let Some((offset, length)) = linux_wal_reservation_range(
        metadata.len(),
        metadata.blocks().saturating_mul(512),
        additional,
        empty_only,
    ) else {
        return Ok(false);
    };
    let offset = libc::off_t::try_from(offset)
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let length = libc::off_t::try_from(length)
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // KEEP_SIZE never pads logical EOF; only reserve a bounded future range.
    let rc = unsafe { libc::fallocate(log.as_raw_fd(), libc::FALLOC_FL_KEEP_SIZE, offset, length) };
    if rc == 0 {
        Ok(true)
    } else {
        Err(io::Error::last_os_error())
    }
}

