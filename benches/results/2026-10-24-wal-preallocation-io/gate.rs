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


#[cfg(target_vendor="apple")]
fn preallocate(file:&File)->io::Result<i64> {
 use std::os::unix::io::AsRawFd;
 let mut request=libc::fstore_t{fst_flags:libc::F_ALLOCATEALL,fst_posmode:libc::F_PEOFPOSMODE,fst_offset:0,fst_length:1024*1024,fst_bytesalloc:0};
 let rc=unsafe{libc::fcntl(file.as_raw_fd(),libc::F_PREALLOCATE,&mut request)};
 if rc==0 {Ok(request.fst_bytesalloc)} else {Err(io::Error::last_os_error())}
}
#[cfg(not(target_vendor="apple"))]
fn preallocate(_: &File)->io::Result<i64>{Err(io::Error::new(io::ErrorKind::Unsupported,"Apple-only diagnostic"))}
fn main(){
 let args:Vec<String>=std::env::args().collect();let data=std::fs::read(&args[1]).unwrap();let root=std::path::Path::new(&args[2]);std::fs::create_dir(root).unwrap();let process:usize=args[3].parse().unwrap();
 let mut buffers:Vec<Vec<u8>>=(0..3).map(|_|Vec::with_capacity(data.len()+4096)).collect();let mut ns=vec![Vec::<u128>::new();3];let mut sizes=[0usize;3];let mut reserve_ns=Vec::new();let mut reserve_bytes=Vec::new();let mut blocks=Vec::new();
 for round in 0..24 {for pos in 0..3 {
  let step=if (round+process)%2==0 {pos}else{2-pos};let v=(step+round+process)%3;
  let path=root.join(format!("{round}-{v}.bin"));let mut f=OpenOptions::new().create_new(true).append(true).open(&path).unwrap();let buf=&mut buffers[v];
  if v==2 {let t=Instant::now();let allocated=preallocate(&f).unwrap();reserve_ns.push(t.elapsed().as_nanos());reserve_bytes.push(allocated);assert_eq!(f.metadata().unwrap().len(),0);}
  use std::os::unix::fs::MetadataExt;
  blocks.push((v,f.metadata().unwrap().blocks()));
  let start=Instant::now();
  let payload=data.as_slice();let codec=3u8;
  let mut h=crc::Hasher::new();h.update(&[codec]);h.update(payload);let checksum=h.finalize();
  buf.clear();buf.extend_from_slice(b"LIN\x06");buf.extend_from_slice(&((payload.len()+5) as u32).to_le_bytes());buf.push(codec);buf.extend_from_slice(payload);buf.extend_from_slice(&checksum.to_le_bytes());
  f.write_all(buf).unwrap();durable_sync(&f).unwrap();let elapsed=start.elapsed().as_nanos();
  ns[v].push(elapsed);sizes[v]=buf.len();drop(f);
  let actual=std::fs::read(&path).unwrap();assert_eq!(actual,*buf);let payload=&actual[9..actual.len()-4];
  assert_eq!(payload,data);assert_eq!(actual.len(),data.len()+13);
  std::fs::remove_file(path).unwrap();
 }}
 std::fs::remove_dir(root).unwrap();println!("{}",serde_json::json!({"process":process,"variants":["raw_A","raw_B","preallocated_raw"],"ns":ns,"frame_bytes":sizes,"exact_roundtrip":true,"sync":"copied production durable_sync; route not instrumented","preallocate_ns":reserve_ns,"allocated_bytes":reserve_bytes,"blocks_before_write":blocks,"logical_length_preserved":true}));
}
