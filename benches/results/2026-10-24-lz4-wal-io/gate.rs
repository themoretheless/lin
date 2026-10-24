use std::{fs::{File,OpenOptions},io::{self,Write},time::Instant};
#[path="../src/crc.rs"] mod crc;
pub(crate) fn durable_sync(file: &File) -> io::Result<()> {
    #[cfg(target_vendor = "apple")]
    {
        use std::os::unix::io::AsRawFd;
        // sys/fcntl.h — not always in libc crate bindings.
        const F_BARRIERFSYNC: libc::c_int = 85;
        let rc = unsafe { libc::fcntl(file.as_raw_fd(), F_BARRIERFSYNC) };
        if rc == 0 {
            return Ok(());
        }
        // Older kernels: fall back to FULLFSYNC via sync_data.
        file.sync_data()
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;
        let rc = unsafe { libc::fdatasync(file.as_raw_fd()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
    {
        file.sync_data()
    }
}

fn main(){
 let args:Vec<String>=std::env::args().collect();let data=std::fs::read(&args[1]).unwrap();let root=std::path::Path::new(&args[2]);std::fs::create_dir(root).unwrap();let process:usize=args[3].parse().unwrap();
 let mut buffers:Vec<Vec<u8>>=(0..3).map(|_|Vec::with_capacity(data.len()+4096)).collect();let mut ns=vec![Vec::<u128>::new();3];let mut sizes=[0usize;3];
 for round in 0..24 {for pos in 0..3 {
  let step=if (round+process)%2==0 {pos}else{2-pos};let v=(step+round+process)%3;
  let path=root.join(format!("{round}-{v}.bin"));let mut f=OpenOptions::new().create_new(true).append(true).open(&path).unwrap();let buf=&mut buffers[v];
  let start=Instant::now();
  let compressed=if v==2 {Some(lz4::block::compress(&data,Some(lz4::block::CompressionMode::FAST(8)),true).unwrap())}else{None};
  let payload=compressed.as_deref().unwrap_or(&data);let codec=if v==2 {0u8}else{3};
  let mut h=crc::Hasher::new();h.update(&[codec]);h.update(payload);let checksum=h.finalize();
  buf.clear();buf.extend_from_slice(b"LIN\x06");buf.extend_from_slice(&((payload.len()+5) as u32).to_le_bytes());buf.push(codec);buf.extend_from_slice(payload);buf.extend_from_slice(&checksum.to_le_bytes());
  f.write_all(buf).unwrap();durable_sync(&f).unwrap();let elapsed=start.elapsed().as_nanos();
  ns[v].push(elapsed);sizes[v]=buf.len();drop(f);
  let actual=std::fs::read(&path).unwrap();assert_eq!(actual,*buf);let payload=&actual[9..actual.len()-4];
  if v==2 {assert_eq!(lz4::block::decompress(payload,None).unwrap(),data)}else{assert_eq!(payload,data)}
  std::fs::remove_file(path).unwrap();
 }}
 std::fs::remove_dir(root).unwrap();println!("{}",serde_json::json!({"process":process,"variants":["raw_A","raw_B","lz4_fast8"],"ns":ns,"frame_bytes":sizes,"exact_roundtrip":true,"sync":"copied production durable_sync; route not instrumented","experimental_codec":0}));
}
