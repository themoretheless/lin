use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
const MAX_RECORD: u32 = 16 * 1024 * 1024;
include!("helper.rs");
include!("reserve.rs");
include!("model.rs");
fn main() {}
#[cfg(test)]
mod native_fs {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    const M: usize = 1024 * 1024;
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("lin-keep-native-{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir(&p).unwrap(); Self(p)
        }
        fn file(&self) -> File {OpenOptions::new().read(true).write(true).create_new(true).open(self.0.join("file")).unwrap()}
    }
    impl Drop for Scratch {fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
    #[test]
    fn keep_size_preserves_cursor_and_bytes() {
        let d=Scratch::new();let mut f=d.file();assert!(try_reserve_wal_space_linux(&f,M,false).unwrap());
        assert_eq!(f.metadata().unwrap().len(),0);assert_eq!(f.stream_position().unwrap(),0);assert!(f.metadata().unwrap().blocks()>0);
        f.write_all(b"prefix").unwrap();f.seek(SeekFrom::Start(2)).unwrap();assert!(try_reserve_wal_space_linux(&f,2*M,false).unwrap());
        assert_eq!(f.metadata().unwrap().len(),6);assert_eq!(f.stream_position().unwrap(),2);assert_eq!(fs::read(d.0.join("file")).unwrap(),b"prefix");
        println!("LIN_KEEP_NATIVE {{\"case\":\"eof_cursor\",\"logical\":6,\"position\":2,\"allocated_bytes\":{}}}",f.metadata().unwrap().blocks()*512);
    }
    #[test]
    fn read_only_fallback_retains_file() {
        let d=Scratch::new();let mut f=d.file();f.write_all(b"prefix").unwrap();drop(f);let mut ro=File::open(d.0.join("file")).unwrap();ro.seek(SeekFrom::Start(3)).unwrap();
        assert!(try_reserve_wal_space_linux(&ro,2*M,false).is_err());reserve_wal_space(&ro,2*M,false);assert_eq!(ro.stream_position().unwrap(),3);assert_eq!(fs::read(d.0.join("file")).unwrap(),b"prefix");
    }
    #[test]
    fn sparse_range_stays_bounded() {
        let d=Scratch::new();let mut f=d.file();let logical=(16*M+7)as u64;f.set_len(logical).unwrap();f.seek(SeekFrom::Start(2)).unwrap();let before=f.metadata().unwrap().blocks()*512;
        assert!(try_reserve_wal_space_linux(&f,M,false).unwrap());let after=f.metadata().unwrap().blocks()*512;assert_eq!(f.metadata().unwrap().len(),logical);assert_eq!(f.stream_position().unwrap(),2);assert!(after<=before+2*M as u64);
        println!("LIN_KEEP_NATIVE {{\"case\":\"sparse_future\",\"logical\":{logical},\"allocated_before\":{before},\"allocated_after\":{after}}}");
    }
    #[test]
    fn truncate_observation() {
        let d=Scratch::new();let mut f=d.file();assert!(try_reserve_wal_space_linux(&f,M,false).unwrap());let before=f.metadata().unwrap().blocks()*512;f.set_len(0).unwrap();f.seek(SeekFrom::End(0)).unwrap();f.sync_all().unwrap();let after=f.metadata().unwrap().blocks()*512;assert_eq!(f.metadata().unwrap().len(),0);assert_eq!(f.stream_position().unwrap(),0);
        println!("LIN_KEEP_NATIVE {{\"case\":\"same_eof_truncate\",\"allocated_before\":{before},\"allocated_after\":{after}}}");
    }
}
