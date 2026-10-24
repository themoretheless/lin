use lin::{Cell,Db};
use std::{sync::Arc,hint::black_box,time::Instant};
type Error=String;
fn io_err(s:&str)->Error{s.to_owned()}
fn append_sparse_rows_serial(
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
            for (offset, value) in values.iter().enumerate() {
                let bits = value.to_bits();
                if bits != 0 {
                    let index = (block * 8 + offset) as u32;
                    let mut entry = [0u8; 8];
                    entry[..4].copy_from_slice(&index.to_le_bytes());
                    entry[4..].copy_from_slice(&bits.to_le_bytes());
                    buf.extend_from_slice(&entry);
                    count += 1;
                }
            }
        }
        buf[count_offset..count_offset + 4].copy_from_slice(&count.to_le_bytes());
    }
    Ok(())
}
fn delta(
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
        let mut previous_plus_one=0u32;
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
            for (offset, value) in values.iter().enumerate() {
                let bits = value.to_bits();
                if bits != 0 {
                    let index = (block * 8 + offset) as u32;
                    let plus_one=index+1;
                    let mut gap=plus_one-previous_plus_one;previous_plus_one=plus_one;
                    while gap>=128 {buf.push((gap as u8 &127)|128);gap>>=7;}
                    buf.push(gap as u8);buf.extend_from_slice(&bits.to_le_bytes());
                    count += 1;
                }
            }
        }
        buf[count_offset..count_offset + 4].copy_from_slice(&count.to_le_bytes());
    }
    Ok(())
}
fn fixture(n:usize) -> Vec<Option<Arc<[f32]>>> {
    let source=format!("insert docs [{}]",(0..n).map(|i|{
        let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
        let wing=if i%2==0 {"rag"}else{"sys"};
        format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000}else{30*86400000})
    }).collect::<Vec<_>>().join(","));
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();assert_eq!(db.run(&source).unwrap().done.n,n);
    db.store.collection("docs").iter().map(|r|{let Cell::Vec(v)=&r["embedding"] else {panic!("embedding")};assert_eq!(v.len(),768);Some(v.clone())}).collect()
}
fn restore_legacy(bytes:&[u8],rows:usize)->Vec<u8>{
 let mut at=0usize;let mut out=Vec::new();
 fn word(bytes:&[u8],at:&mut usize)->u32{let x=u32::from_le_bytes(bytes[*at..*at+4].try_into().unwrap());*at+=4;x}
 for _ in 0..rows{let dim=word(bytes,&mut at);out.extend_from_slice(&dim.to_le_bytes());if dim==0{continue;}let count=word(bytes,&mut at);out.extend_from_slice(&count.to_le_bytes());let mut previous=0u32;
 for _ in 0..count {let mut value=0u32;let mut shift=0;loop{assert!(shift<=28);let b=bytes[at];at+=1;if shift==28{assert!(b<=15);}value|=((b&127)as u32)<<shift;if b&128==0{break;}shift+=7;}
 assert!(value>0);previous=previous.checked_add(value).unwrap();let index=previous-1;assert!(index<dim-1);out.extend_from_slice(&index.to_le_bytes());let bits=word(bytes,&mut at);out.extend_from_slice(&bits.to_le_bytes());}
 }assert_eq!(at,bytes.len());out
}
use std::{fs::{File,OpenOptions},io::{self,Write}};
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

fn main(){
 let args:Vec<String>=std::env::args().collect();let data=std::fs::read(&args[1]).unwrap();let root=std::path::Path::new(&args[2]);std::fs::create_dir(root).unwrap();let process:usize=args[3].parse().unwrap();
 let rows=fixture(1000);let mut legacy=Vec::new();append_sparse_rows_serial(&rows,&mut legacy).unwrap();let matches:Vec<usize>=data.windows(legacy.len()).enumerate().filter(|(_,x)|*x==legacy.as_slice()).map(|(i,_)|i).collect();assert_eq!(matches.len(),1);let offset=matches[0];
 let mut small=Vec::new();delta(&rows,&mut small).unwrap();assert_eq!(restore_legacy(&small,rows.len()),legacy);
 let mut ns=vec![Vec::<u128>::new();3];let mut reserve_setup=vec![Vec::<u128>::new();3];let mut frame_sizes=[0usize;3];let mut vectors=Vec::with_capacity(legacy.len());let mut payload=Vec::with_capacity(data.len());let mut frame=Vec::with_capacity(data.len()+13);
 for round in 0..24{for pos in 0..3{let step=if(round+process)%2==0{pos}else{2-pos};let v=(step+round+process)%3;let path=root.join(format!("{round}-{v}.bin"));let mut f=OpenOptions::new().create_new(true).read(true).write(true).open(&path).unwrap();
 let t=Instant::now();reserve_wal_space(&f,1024*1024,true);reserve_setup[v].push(t.elapsed().as_nanos());assert_eq!(f.metadata().unwrap().len(),0);
 let start=Instant::now();vectors.clear();if v==2{delta(&rows,&mut vectors).unwrap();}else{append_sparse_rows_serial(&rows,&mut vectors).unwrap();}
 payload.clear();payload.extend_from_slice(&data[..offset]);payload.extend_from_slice(&vectors);payload.extend_from_slice(&data[offset+legacy.len()..]);
 // Experimental marker 0 is confined to disposable diagnostic files; no reader contract.
 let codec=if v==2{0u8}else{3u8};let mut h=crc::Hasher::new();h.update(&[codec]);h.update(&payload);let checksum=h.finalize();frame.clear();frame.extend_from_slice(b"LIN\x06");frame.extend_from_slice(&((payload.len()+5)as u32).to_le_bytes());frame.push(codec);frame.extend_from_slice(&payload);frame.extend_from_slice(&checksum.to_le_bytes());
 reserve_wal_space(&f,payload.len()+13,false);f.write_all(&frame).unwrap();durable_sync(&f).unwrap();ns[v].push(start.elapsed().as_nanos());frame_sizes[v]=frame.len();drop(f);
 let disk=std::fs::read(&path).unwrap();assert_eq!(disk,frame);let actual=&disk[9..disk.len()-4];if v==2{let restored=restore_legacy(&actual[offset..offset+small.len()],rows.len());assert_eq!(restored,legacy);assert_eq!(&actual[..offset],&data[..offset]);assert_eq!(&actual[offset+small.len()..],&data[offset+legacy.len()..]);}else{assert_eq!(actual,data);}
 std::fs::remove_file(path).unwrap();}}
 std::fs::remove_dir(root).unwrap();println!("{}",serde_json::json!({"process":process,"ns":ns,"frame_bytes":frame_sizes,"vector_bytes":[legacy.len(),small.len()],"vector_offset":offset,"reserve_setup_ns":reserve_setup,"exact_roundtrip":true,"experimental_codec":0,"variants":["legacy_A","legacy_B","delta"],"timed":"vector encode, payload splice, checksum, frame copy, reserve check, write, production Full sync"}));
}
