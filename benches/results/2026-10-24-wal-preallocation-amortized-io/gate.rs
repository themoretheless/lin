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
 let paths:Vec<_>=(0..3).map(|v|root.join(format!("{v}.bin"))).collect();
 let mut files:Vec<Option<File>>=(0..3).map(|_|None).collect();
 for pos in 0..3 {let v=(pos+process)%3;files[v]=Some(OpenOptions::new().create_new(true).append(true).open(&paths[v]).unwrap());}
 let mut buffers:Vec<Vec<u8>>=(0..3).map(|_|Vec::with_capacity(data.len()+4096)).collect();let mut ns=vec![Vec::<u128>::new();3];let mut reserve_ns=Vec::new();let mut allocated=Vec::new();let mut capacity=0usize;let mut length=0usize;
 for round in 0..32 {for pos in 0..3 {
  let step=if (round+process)%2==0 {pos}else{2-pos};let v=(step+round+process)%3;let f=files[v].as_mut().unwrap();let buf=&mut buffers[v];
  let start=Instant::now();
  if v==2 && length+data.len()+13>capacity {
   let before=f.metadata().unwrap().len();let t=Instant::now();let bytes=preallocate(f).unwrap();let dt=t.elapsed().as_nanos();assert!(bytes>0);capacity+=bytes as usize;assert_eq!(f.metadata().unwrap().len(),before);reserve_ns.push(dt);allocated.push(bytes);
  }
  let mut h=crc::Hasher::new();h.update(&[3]);h.update(&data);let checksum=h.finalize();
  buf.clear();buf.extend_from_slice(b"LIN\x06");buf.extend_from_slice(&((data.len()+5) as u32).to_le_bytes());buf.push(3);buf.extend_from_slice(&data);buf.extend_from_slice(&checksum.to_le_bytes());
  f.write_all(buf).unwrap();durable_sync(f).unwrap();ns[v].push(start.elapsed().as_nanos());
  if v==2 {length+=buf.len();}
 }}
 drop(files);
 for path in &paths {let actual=std::fs::read(path).unwrap();assert_eq!(actual.len(),32*(data.len()+13));for frame in actual.chunks_exact(data.len()+13) {assert_eq!(&frame[9..frame.len()-4],data);let mut h=crc::Hasher::new();h.update(&frame[8..frame.len()-4]);assert_eq!(h.finalize().to_le_bytes().as_slice(),&frame[frame.len()-4..]);}std::fs::remove_file(path).unwrap();}
 std::fs::remove_dir(root).unwrap();println!("{}",serde_json::json!({"process":process,"variants":["raw_A","raw_B","preallocated_raw"],"ns":ns,"preallocate_ns":reserve_ns,"allocated_bytes":allocated,"logical_length_preserved":true,"exact_roundtrip":true,"preallocation_in_timing":true,"appends":32}));
}
