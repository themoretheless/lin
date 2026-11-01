use std::{env,fs::{self,OpenOptions},io::{Read,Seek,SeekFrom,Write},path::PathBuf,time::Instant};
fn main()->std::io::Result<()> {
 let a:Vec<String>=env::args().collect();assert_eq!(a.len(),5);
 let dir=PathBuf::from(&a[1]);let size:usize=a[2].parse().unwrap();let offset:usize=a[3].parse().unwrap();let samples:usize=a[4].parse().unwrap();
 assert!([117,4095,4096,4097,4125,8192].contains(&size));assert!([0,2048,4095].contains(&offset));assert!([1,8].contains(&samples));fs::create_dir(&dir)?;
 let prefix=vec![0x35u8;offset];let payload=vec![0xa7u8;size];
 for sample in 0..samples {
  let path=dir.join(format!("sample-{sample}.wal"));let mut f=OpenOptions::new().read(true).write(true).create_new(true).open(&path)?;
  f.write_all(&prefix)?;f.sync_data()?;assert_eq!(f.stream_position()?,offset as u64);
  let start=Instant::now();f.write_all(&payload)?;let write_ns=start.elapsed().as_nanos();let sync=Instant::now();f.sync_data()?;let sync_ns=sync.elapsed().as_nanos();let total_ns=start.elapsed().as_nanos();
  assert_eq!(f.metadata()?.len(),(offset+size)as u64);f.seek(SeekFrom::Start(0))?;let mut actual=Vec::new();f.read_to_end(&mut actual)?;assert_eq!(&actual[..offset],&prefix);assert_eq!(&actual[offset..],&payload);
  drop(f);let reopened=fs::read(&path)?;assert_eq!(actual,reopened);
  println!("{{\"sample\":{sample},\"size\":{size},\"offset\":{offset},\"write_ns\":{write_ns},\"sync_ns\":{sync_ns},\"total_ns\":{total_ns},\"validated_bytes\":{}}}",offset+size);
 }
 Ok(())
}
