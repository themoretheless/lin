//! Physical I/O diagnostic only; not a WAL implementation or recovery format.
use std::{fs::OpenOptions, io::{Read, Seek, SeekFrom, Write}, os::unix::fs::FileExt, path::PathBuf, time::Instant};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 3);
    let mode = args[1].as_str();
    assert!(["append", "fixed_sparse", "fixed_zeroed"].contains(&mode));
    let path = PathBuf::from(&args[2]);
    let mut file = OpenOptions::new().read(true).write(true).create_new(true).open(&path).unwrap();
    let prefix = 16 * 1024u64;
    let physical = 8 * 1024 * 1024u64;
    let samples = 64usize;
    let payload: Vec<u8> = (0..4125).map(|i| (i % 251) as u8).collect();
    file.write_all(&vec![0; prefix as usize]).unwrap();
    match mode {
        "fixed_sparse" => file.set_len(physical).unwrap(),
        "fixed_zeroed" => {
            let zeroes = vec![0; 64 * 1024];
            while file.stream_position().unwrap() < physical { file.write_all(&zeroes).unwrap(); }
            file.set_len(physical).unwrap();
        }
        _ => {}
    }
    file.sync_all().unwrap();
    let mut times = Vec::with_capacity(samples);
    for i in 0..samples {
        let offset = prefix + i as u64 * payload.len() as u64;
        let end = offset + payload.len() as u64;
        let mut header = [0u8; 16];
        header[..8].copy_from_slice(&end.to_le_bytes());
        header[8..].copy_from_slice(&(!end).to_le_bytes());
        let start = Instant::now();
        file.write_all_at(&payload, offset).unwrap();
        if mode != "append" { file.write_all_at(&header, 0).unwrap(); }
        file.sync_data().unwrap();
        times.push(start.elapsed().as_nanos());
    }
    let end = prefix + samples as u64 * payload.len() as u64;
    assert_eq!(file.metadata().unwrap().len(), if mode == "append" {end} else {physical});
    drop(file);
    let mut reopened = OpenOptions::new().read(true).open(&path).unwrap();
    let mut got = vec![0; payload.len()];
    for i in 0..samples {
        reopened.seek(SeekFrom::Start(prefix + i as u64 * payload.len() as u64)).unwrap();
        reopened.read_exact(&mut got).unwrap();
        assert_eq!(got, payload);
    }
    if mode != "append" {
        let mut header = [0u8; 16];
        reopened.seek(SeekFrom::Start(0)).unwrap();reopened.read_exact(&mut header).unwrap();
        assert_eq!(&header[..8],&end.to_le_bytes());assert_eq!(&header[8..],&(!end).to_le_bytes());
    }
    let length = reopened.metadata().unwrap().len();drop(reopened);
    // Remove only the create_new file owned by this process, after verifying readback.
    std::fs::remove_file(&path).unwrap();
    println!("{{\"mode\":\"{mode}\",\"samples\":{samples},\"payload_bytes\":{},\"physical_bytes\":{length},\"logical_end\":{end},\"full_readback_verified\":true,\"owned_file_removed\":true,\"latencies_ns\":{:?}}}",payload.len(),times);
}
