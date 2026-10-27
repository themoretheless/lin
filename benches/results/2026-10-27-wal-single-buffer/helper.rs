
// Private scratch-buffer entry point; public append_record retains its buffer behavior.
pub(crate) fn append_record_buffered(log: &mut File, rec: &LogRecord, buf: &mut Vec<u8>, sync: SyncMode, log_bytes: &mut u64) -> Result<(), Error> {
    if matches!(&rec.pack, Pack::AppendFactsBulk { .. } | Pack::InsertCols { .. }) {
        return append_record(log, rec, buf, sync, log_bytes);
    }
    buf.clear();
    buf.extend_from_slice(&[0; 9]);
    rmp_serde::encode::write_named(&mut *buf, rec).map_err(io_err)?;
    let payload_len = buf.len() - 9;
    let len = payload_len.checked_add(5).filter(|&n| n <= MAX_RECORD as usize).ok_or_else(|| io_err("log record exceeds 16MiB"))?;
    buf[..4].copy_from_slice(&LOG_MAGIC_CHECKSUM);
    buf[4..8].copy_from_slice(&(len as u32).to_le_bytes());
    buf[8] = 1;
    let mut checksum = crate::crc::Hasher::new();
    checksum.update(&buf[8..]);
    buf.extend_from_slice(&checksum.finalize().to_le_bytes());
    if payload_len >= 64 * 1024 { reserve_wal_space(log, payload_len.saturating_add(13), false); }
    log.write_all(buf).map_err(io_err)?;
    if sync == SyncMode::Full { durable_sync(log).map_err(io_err)?; }
    *log_bytes += 8 + len as u64;
    Ok(())
}
