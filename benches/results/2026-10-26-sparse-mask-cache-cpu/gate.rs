use lin::{Cell,Db,Row};
use std::{sync::Arc,time::Instant,hint::black_box,io};
type Error=String;fn io_err(s:&str)->Error{s.to_owned()}
const SPARSE_VECTOR_THRESHOLD:usize=64*1024;
fn use_sparse_vectors(rows: &[Option<std::sync::Arc<[f32]>>]) -> bool {
    let dense = rows
        .iter()
        .map(|row| 4usize.saturating_add(row.as_ref().map_or(0, |v| v.len().saturating_mul(4))))
        .fold(0usize, usize::saturating_add);
    if rows
        .iter()
        .any(|row| row.as_ref().is_some_and(|v| v.is_empty()))
    {
        return true; // Preserve Some(empty), which the legacy vector tag cannot express.
    }
    if dense < SPARSE_VECTOR_THRESHOLD {
        return false;
    }
    if rows.len() >= 4096 && dense >= 8 * 1024 * 1024 {
        return sparse_bytes_parallel(rows) < dense;
    }
    let sparse = rows
        .iter()
        .map(|row| {
            row.as_ref().map_or(4, |v| {
                8usize.saturating_add(
                    v.iter()
                        .filter(|value| value.to_bits() != 0)
                        .count()
                        .saturating_mul(8),
                )
            })
        })
        .fold(0usize, usize::saturating_add);
    sparse < dense
}

fn sparse_bytes_serial(rows: &[Option<std::sync::Arc<[f32]>>]) -> usize {
    rows.iter()
        .map(|row| {
            row.as_ref().map_or(4, |v| {
                8usize.saturating_add(
                    v.iter()
                        .filter(|value| value.to_bits() != 0)
                        .count()
                        .saturating_mul(8),
                )
            })
        })
        .fold(0usize, usize::saturating_add)
}

fn sparse_count_worker(
    rows: &[Option<std::sync::Arc<[f32]>>],
    worker: io::Result<std::thread::ScopedJoinHandle<'_, usize>>,
) -> usize {
    match worker {
        Ok(worker) => worker.join().expect("sparse size worker panicked"),
        Err(_) => sparse_bytes_serial(rows),
    }
}

fn sparse_bytes_parallel(rows: &[Option<std::sync::Arc<[f32]>>]) -> usize {
    std::thread::scope(|scope| {
        let chunk_len = rows.len().div_ceil(4);
        let (left, rest) = rows.split_at(chunk_len);
        let mut workers = Vec::with_capacity(3);
        for right in rest.chunks(chunk_len.max(1)) {
            let worker =
                std::thread::Builder::new().spawn_scoped(scope, move || sparse_bytes_serial(right));
            workers.push((right, worker));
        }
        let mut size = sparse_bytes_serial(left);
        for (right, worker) in workers {
            size = size.saturating_add(sparse_count_worker(right, worker));
        }
        size
    })
}

fn append_sparse_rows_serial(
    rows: &[Option<std::sync::Arc<[f32]>>],
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    #[cfg(target_arch = "aarch64")]
    if std::arch::is_aarch64_feature_detected!("neon") {
        // Runtime feature check covers all NEON loads; helper only loads full eight-value chunks.
        return unsafe { append_sparse_rows_neon(rows, buf) };
    }
    append_sparse_rows_scalar(rows, buf)
}

fn append_sparse_rows_scalar(
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

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn presence_mask8(values: &[f32]) -> u32 {
    assert_eq!(values.len(), 8);
    use std::arch::aarch64::*;
    unsafe {
        let a = vreinterpretq_u32_f32(vld1q_f32(values.as_ptr()));
        let b = vreinterpretq_u32_f32(vld1q_f32(values.as_ptr().add(4)));
        if vmaxvq_u32(vorrq_u32(a, b)) == 0 {
            return 0;
        }
        let weights = vld1q_u32([1u32, 2, 4, 8].as_ptr());
        let zero = vdupq_n_u32(0);
        let lo = vaddvq_u32(vandq_u32(vmvnq_u32(vceqq_u32(a, zero)), weights));
        let hi = vaddvq_u32(vandq_u32(vmvnq_u32(vceqq_u32(b, zero)), weights));
        lo | (hi << 4)
    }
}
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn append_sparse_rows_neon(
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
            let mut mask = if values.len() == 8 {
                unsafe { presence_mask8(values) }
            } else {
                values
                    .iter()
                    .enumerate()
                    .fold(0u32, |m, (i, x)| m | (u32::from(x.to_bits() != 0) << i))
            };
            while mask != 0 {
                let offset = mask.trailing_zeros() as usize;
                let index = (block * 8 + offset) as u32;
                let mut entry = [0u8; 8];
                entry[..4].copy_from_slice(&index.to_le_bytes());
                entry[4..].copy_from_slice(&values[offset].to_bits().to_le_bytes());
                buf.extend_from_slice(&entry);
                count += 1;
                mask &= mask - 1;
            }
        }
        buf[count_offset..count_offset + 4].copy_from_slice(&count.to_le_bytes());
    }
    Ok(())
}

fn append_sparse_worker(
    buf: &mut Vec<u8>,
    right: &[Option<std::sync::Arc<[f32]>>],
    worker: io::Result<std::thread::ScopedJoinHandle<'_, Result<Vec<u8>, Error>>>,
) -> Result<(), Error> {
    match worker {
        Ok(worker) => buf.extend_from_slice(&worker.join().expect("sparse WAL worker panicked")?),
        Err(_) => append_sparse_rows_serial(right, buf)?,
    }
    Ok(())
}

fn append_sparse_rows_parallel(
    rows: &[Option<std::sync::Arc<[f32]>>],
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    std::thread::scope(|scope| {
        let chunk_len = rows.len().div_ceil(4);
        let (left, rest) = rows.split_at(chunk_len);
        let mut workers = Vec::with_capacity(3);
        for right in rest.chunks(chunk_len.max(1)) {
            let worker = std::thread::Builder::new().spawn_scoped(scope, move || {
                let mut bytes = Vec::with_capacity(right.len() * 32);
                append_sparse_rows_serial(right, &mut bytes)?;
                Ok(bytes)
            });
            workers.push((right, worker));
        }
        append_sparse_rows_serial(left, buf)?;
        for (right, worker) in workers {
            append_sparse_worker(buf, right, worker)?;
        }
        Ok(())
    })
}


struct Cache { sparse:bool, masks:Vec<u8> }
fn cached_classify(rows:&[Option<Arc<[f32]>>])->Option<Cache>{
 let dense=rows.iter().map(|r|4usize.saturating_add(r.as_ref().map_or(0,|v|v.len().saturating_mul(4)))).fold(0usize,usize::saturating_add);
 let empty=rows.iter().any(|r|r.as_ref().is_some_and(|v|v.is_empty()));
 if dense<SPARSE_VECTOR_THRESHOLD && !empty{return Some(Cache{sparse:false,masks:Vec::new()});}
 let capacity=rows.iter().flatten().map(|v|v.len().div_ceil(8)).fold(0usize,usize::saturating_add);
 if capacity>512*1024 {return None;}
 let mut masks=Vec::with_capacity(capacity);let mut sparse=0usize;
 for row in rows {
  let Some(v)=row else {sparse=sparse.saturating_add(4);continue;};sparse=sparse.saturating_add(8);
  for chunk in v.chunks(8){
   let mask=if chunk.len()==8 {unsafe{presence_mask8(chunk)}}else{chunk.iter().enumerate().fold(0u32,|m,(i,x)|m | (u32::from(x.to_bits()!=0)<<i))};
   masks.push(mask as u8);sparse=sparse.saturating_add((mask.count_ones() as usize)*8);
  }
 }
 Some(Cache{sparse:empty || sparse<dense,masks})
}
fn append_cached(rows:&[Option<Arc<[f32]>>],masks:&[u8],out:&mut Vec<u8>)->Result<(),Error>{
 let mut masks=masks.iter();
 for row in rows {
  let Some(v)=row else {out.extend_from_slice(&0u32.to_le_bytes());continue;};
  let dim=u32::try_from(v.len()).ok().and_then(|n|n.checked_add(1)).ok_or_else(||io_err("sparse vector dimension overflow"))?;
  out.extend_from_slice(&dim.to_le_bytes());let at=out.len();out.extend_from_slice(&0u32.to_le_bytes());let mut count=0u32;
  for (block,chunk)in v.chunks(8).enumerate(){let mut mask=*masks.next().unwrap() as u32;
   while mask!=0 {let i=mask.trailing_zeros() as usize;let index=(block*8+i)as u32;let mut entry=[0u8;8];entry[..4].copy_from_slice(&index.to_le_bytes());entry[4..].copy_from_slice(&chunk[i].to_bits().to_le_bytes());out.extend_from_slice(&entry);count+=1;mask&=mask-1;}
  }
  out[at..at+4].copy_from_slice(&count.to_le_bytes());
 }
 assert!(masks.next().is_none());Ok(())
}
fn dense_write(rows:&[Option<Arc<[f32]>>],out:&mut Vec<u8>){for row in rows {let v=row.as_deref().unwrap_or(&[]);out.extend_from_slice(&(v.len() as u32).to_le_bytes());
 #[cfg(target_endian="little")] {let bytes=unsafe{std::slice::from_raw_parts(v.as_ptr() as *const u8,v.len()*std::mem::size_of::<f32>())};out.extend_from_slice(bytes);}
 #[cfg(not(target_endian="little"))] {for x in v{out.extend_from_slice(&x.to_le_bytes());}}
}}
fn encode(rows:&[Option<Arc<[f32]>>],mode:usize)->(bool,Vec<u8>,usize){
 let cached=if mode==2 && (512..4096).contains(&rows.len()) {cached_classify(rows)}else{None};
 let sparse=cached.as_ref().map_or_else(||use_sparse_vectors(rows),|c|c.sparse);
 let mut out=Vec::with_capacity(rows.len()*(8+768*4)+64);let bytes=cached.as_ref().map_or(0,|c|c.masks.len());
 if sparse {
  if let Some(cache)=cached {append_cached(rows,&cache.masks,&mut out).unwrap();}
  else if rows.len()>=4096 && rows.iter().flatten().map(|v|v.len()).sum::<usize>()>=2*1024*1024 {append_sparse_rows_parallel(rows,&mut out).unwrap();}
  else {append_sparse_rows_serial(rows,&mut out).unwrap();}
 }else {dense_write(rows,&mut out);}
 (sparse,out,bytes)
}
fn oracle(){
 let mut state=0x0badf00d12345678u64;let special=[0u32,0x80000000,0x7f800000,0xff800000,0x7fc01234,0xffc03456,1,0x80000001];
 for offset in 0..4{for len in 0..=1024{
  let mut v=Vec::new();for i in 0..len+offset{state^=state<<13;state^=state>>7;state^=state<<17;v.push(f32::from_bits(if i%3==0{state as u32}else{special[i%special.len()]}));}
  let rows=vec![None,Some(Arc::from([])),Some(Arc::from(&v[offset..]))];let cache=cached_classify(&rows).unwrap();assert_eq!(cache.sparse,use_sparse_vectors(&rows));let mut a=Vec::new();let mut b=Vec::new();append_sparse_rows_scalar(&rows,&mut a).unwrap();append_cached(&rows,&cache.masks,&mut b).unwrap();assert_eq!(a,b);
 }}
 for dim in [16382,16383,16384,16385,20000] {for bits in [0u32,0x80000000,0x3f800000]{let rows=vec![Some(Arc::from(vec![f32::from_bits(bits);dim]))];let cache=cached_classify(&rows).unwrap();assert_eq!(cache.sparse,use_sparse_vectors(&rows));}}
 let rows=vec![Some(Arc::from(vec![0f32;4*1024*1024+1]))];assert!(cached_classify(&rows).is_none());
}
fn fixture(n:usize) -> Vec<Row> {
    let source=format!("insert docs [{}]",(0..n).map(|i|{
        let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
        let wing=if i%2==0 {"rag"}else{"sys"};
        format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000}else{30*86400000})
    }).collect::<Vec<_>>().join(","));
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();assert_eq!(db.run(&source).unwrap().done.n,n);
    db.store.collection("docs").to_vec()
}

fn main(){let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();assert!(std::arch::is_aarch64_feature_detected!("neon"));oracle();
 let actual=|rows:&[Row]| rows.iter().map(|r|match r.get("embedding"){Some(Cell::Vec(v))=>Some(v.clone()),_=>None}).collect::<Vec<_>>();
 let mut half=vec![0f32;768];for i in 0..383{half[i*768/383]=1.0;}
 let cases=vec![("default_1k",actual(&fixture(1000))),("large_10k_fallback",actual(&fixture(10000))),("small_127_fallback",actual(&fixture(127))),("zero_1k",vec![Some(Arc::from(vec![0f32;768]));1000]),("half_1k",vec![Some(Arc::from(half));1000]),("dense_1k",vec![Some(Arc::from(vec![1f32;768]));1000])];
 for(case,rows)in cases {let expected=encode(&rows,0);let mut ns=vec![Vec::<u128>::new();3];let mut cache_bytes=0;
 for round in 0..24 {for pos in 0..3{let mode=(process+round+pos)%3;let timer=Instant::now();let(sparse,bytes,used)=encode(black_box(&rows),mode);ns[mode].push(timer.elapsed().as_nanos());assert_eq!(sparse,expected.0);assert_eq!(bytes,expected.1);cache_bytes=cache_bytes.max(used);drop(bytes);}}
 println!("{}",serde_json::json!({"case":case,"rows":rows.len(),"ns":ns,"bytes":expected.1.len(),"sparse":expected.0,"mask_bytes":cache_bytes,"exact_bytes_and_codec":true}));
 }}