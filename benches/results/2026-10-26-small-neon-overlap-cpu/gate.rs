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

fn fixture(n:usize) -> Vec<Row> {
    let source=format!("insert docs [{}]",(0..n).map(|i|{
        let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
        let wing=if i%2==0 {"rag"}else{"sys"};
        format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000}else{30*86400000})
    }).collect::<Vec<_>>().join(","));
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();assert_eq!(db.run(&source).unwrap().done.n,n);
    db.store.collection("docs").to_vec()
}

fn encode(vectors:&[Option<Arc<[f32]>>])->Vec<u8>{
 assert!(use_sparse_vectors(vectors));let mut out=Vec::with_capacity(vectors.len()*(8+768*4)+64);
 if vectors.len()>=4096 && vectors.iter().flatten().map(|v|v.len()).sum::<usize>()>=2*1024*1024{append_sparse_rows_parallel(vectors,&mut out).unwrap();}else{append_sparse_rows_serial(vectors,&mut out).unwrap();}out
}
fn maintain(db:&mut Db,rows:&[Row]){
 db.store.collection_mut("docs").reserve(rows.len());db.store.row_maps_reserve("docs",rows.len());
 db.store.index_insert_slab("docs",0,rows).unwrap();db.store.fts_insert_slab("docs",0,rows);db.store.hnsw_insert_slab("docs",0,rows);db.store.row_maps_register_slab("docs",0,rows);
}

fn vectors(rows:&[Row])->Vec<Option<Arc<[f32]>>>{rows.iter().map(|r|match r.get("embedding"){Some(Cell::Vec(v))=>Some(v.clone()),_=>None}).collect()}
struct Job { rows:Arc<Vec<Row>>, reply:std::sync::mpsc::SyncSender<Vec<u8>> }
struct Worker { tx:Option<std::sync::mpsc::SyncSender<Job>>, handle:Option<std::thread::JoinHandle<()>> }
impl Worker {
 fn new()->Self {let(tx,rx)=std::sync::mpsc::sync_channel::<Job>(1);let handle=std::thread::spawn(move||{
  while let Ok(Job{rows,reply})=rx.recv(){let bytes=encode(&vectors(&rows));drop(rows);if reply.send(bytes).is_err(){break;}}
 });Self{tx:Some(tx),handle:Some(handle)}}
}
impl Drop for Worker {fn drop(&mut self){drop(self.tx.take());self.handle.take().unwrap().join().unwrap();}}
fn execute(db:&mut Db,rows:Vec<Row>,worker:Option<&Worker>)->(Vec<u8>,Vec<Row>,bool){
 if rows.len()<128 || worker.is_none(){maintain(db,&rows);let bytes=encode(&vectors(&rows));return(bytes,rows,false);}
 let rows=Arc::new(rows);let(tx,rx)=std::sync::mpsc::sync_channel(1);
 let used=match worker.unwrap().tx.as_ref().unwrap().try_send(Job{rows:rows.clone(),reply:tx}){
  Ok(())=>true,Err(error)=>{drop(error);false}
 };
 maintain(db,&rows);let bytes=if used {rx.recv().unwrap()}else{encode(&vectors(&rows))};
 let rows=Arc::try_unwrap(rows).expect("worker must release input before reply");(bytes,rows,used)
}
fn verify(db:&mut Db,rows:Vec<Row>,bytes:Vec<u8>,expected:&[u8]){
 assert_eq!(bytes,expected);let n=rows.len();db.store.collection_mut("docs").extend(rows.iter().cloned());
 let actual=db.run(&format!("docs | take {n}")).unwrap().rows;assert_eq!(actual,rows);
 for (a,b) in actual.iter().zip(&rows){for(k,v)in a{if let Cell::Vec(a)=v{if let Some(Cell::Vec(b))=b.get(k){assert!(a.iter().map(|x|x.to_bits()).eq(b.iter().map(|x|x.to_bits())));}}}}
 for i in 0..n{assert_eq!(db.store.row_index("docs",&format!("d-{i}")),Some(i));}
 for (query,step) in [(r#"search lex "wal""#,10),(r#"wing == "rag""#,2)]{
 let got=db.run(&format!("docs | {query} | take {n}")).unwrap().rows;
 let mut got:Vec<String>=got.iter().map(|r|format!("{:?}",r.get("id").unwrap())).collect();got.sort();
 let mut want:Vec<String>=rows.iter().step_by(step).map(|r|format!("{:?}",r.get("id").unwrap())).collect();want.sort();assert_eq!(got,want);
 }
}
fn fallback_checks(){
 // Deterministic disconnected and full queues; retain every input Arc on rejection.
 let(tx,rx)=std::sync::mpsc::sync_channel::<Job>(1);drop(rx);let(reply,_)=std::sync::mpsc::sync_channel(1);
 let rows=Arc::new(Vec::new());let error=tx.try_send(Job{rows:rows.clone(),reply}).unwrap_err();drop(error);assert_eq!(Arc::strong_count(&rows),1);
 let(tx,_rx)=std::sync::mpsc::sync_channel::<Job>(1);let(reply,_)=std::sync::mpsc::sync_channel(1);tx.try_send(Job{rows:rows.clone(),reply}).unwrap();
 let(reply,_)=std::sync::mpsc::sync_channel(1);assert!(matches!(tx.try_send(Job{rows:rows.clone(),reply}),Err(std::sync::mpsc::TrySendError::Full(_))));
}
fn main(){let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();fallback_checks();let worker=Worker::new();
 for(name,n)in[("default_512",512),("default_1k",1000),("large_10k",10000),("small_127",127)]{
  let reference=fixture(n);let expected=encode(&vectors(&reference));let mut ns=vec![Vec::<u128>::new();4];let mut used=vec![0usize;4];
  for round in 0..24 {for pos in 0..4 {let mode=(process+round+pos)%4;let rows=reference.clone();let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();
   let timer=Instant::now();let cold=if mode==3 && n>=128 {Some(Worker::new())}else{None};
   let(bytes,rows,ran)=execute(&mut db,black_box(rows),if mode==2{Some(&worker)}else{cold.as_ref()});
   ns[mode].push(timer.elapsed().as_nanos());used[mode]+=usize::from(ran);verify(&mut db,rows,bytes,&expected);drop(cold);
  }}assert_eq!(used,vec![0,0,if n>=128{24}else{0},if n>=128{24}else{0}]);
  println!("{}",serde_json::json!({"case":name,"rows":n,"ns":ns,"used_worker":used,"vector_bytes":expected.len(),"exact_state_and_bytes":true}));
 }
 drop(worker);
}